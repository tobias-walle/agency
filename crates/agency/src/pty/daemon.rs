//! Daemon: hosts a PTY shell and serves clients over a Unix socket.
//!
//! High-level flow:
//! - Bind a Unix listener and keep it non-blocking so we can interleave child
//!   exit checks with accepting new clients.
//! - Maintain a `Session` running the shell. It survives client disconnects.
//! - Allow a single attached client at a time; reject concurrent attaches.
//! - On attach, send `Hello` + `Snapshot`, then stream PTY output as `D2C::Output`.
//! - On detach or disconnect, stop client threads and allow new attaches.
//! - If the shell exits while a client is attached, send `D2C::Exited`, restart
//!   the shell, and send a fresh `Hello` + `Snapshot`.

use crate::pty::protocol::{
  C2D, C2DControl, D2C, D2CControl, D2CControlChannel, ProjectKey, SessionOpenMeta,
  make_d2c_control_channel, make_output_channel, read_frame, write_frame,
};
use crate::pty::registry::SessionRegistry;
use anyhow::Context;
use log::{debug, error, info, warn};
use parking_lot::Mutex;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub fn run_daemon(socket_path: &Path) -> anyhow::Result<()> {
  info!("Starting daemon. Socket path: {}", socket_path.display());
  // If another daemon is already running, bail early by attempting a connect.
  if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
    warn!("Daemon is already running");
    return Ok(());
  }

  let daemon = Daemon::new(socket_path)?;
  daemon.run()
}

/// Represents the daemon's attachment lifecycle and state management.
///
/// The daemon only allows a single client attachment at a time.
/// When attached, it owns the control and output channels as well as
/// the reader/writer thread handles.
/// Central orchestrator for PTY session and client lifecycle.
///
/// Owns the Unix listener, the `Session`, and explicit `DaemonState` to make
/// transitions and thread ownership clear and documented.
///
/// Invariants:
/// - Never hold `session` or `state` mutex guards while sending frames.
/// - Lock scopes must be short and confined; helpers enforce this pattern.
/// - Control-channel sends occur only after guards are dropped.
pub struct Daemon {
  /// Bound Unix domain socket listener used to accept clients.
  listener: UnixListener,
  /// Global registry of sessions and clients.
  registry: Arc<Mutex<SessionRegistry>>,
  /// Path to the bound Unix socket for cleanup.
  socket_path: PathBuf,
  /// Shutdown flag set when a `Shutdown` control is received.
  shutdown: Arc<std::sync::atomic::AtomicBool>,
  /// Subscribers to control events per project.
  subscribers: Arc<Mutex<Vec<Subscriber>>>,
}

impl Daemon {
  /// Constructs and configures a new `Daemon`.
  ///
  /// Binds the socket at `socket_path`, creates an empty registry,
  /// and sets the listener to non-blocking.
  pub fn new(socket_path: &Path) -> anyhow::Result<Self> {
    let listener = ensure_socket_dir_and_bind(socket_path)?;
    listener.set_nonblocking(true)?;
    Ok(Self {
      listener,
      registry: Arc::new(Mutex::new(SessionRegistry::new())),
      socket_path: socket_path.to_path_buf(),
      shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
      subscribers: Arc::new(Mutex::new(Vec::new())),
    })
  }

  /// Runs the main accept loop interleaving child exit checks with accepts.
  pub fn run(&self) -> anyhow::Result<()> {
    info!("Daemon running");
    // Debug-only deadlock detection loop
    #[cfg(debug_assertions)]
    {
      std::thread::spawn(|| {
        loop {
          std::thread::sleep(std::time::Duration::from_millis(200));
          let deadlocks = parking_lot::deadlock::check_deadlock();
          if !deadlocks.is_empty() {
            error!("Deadlock detected: {} cycles", deadlocks.len());
          }
        }
      });
    }
    while !self.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
      // Handle session exits and broadcast notifications.
      self.poll_session_exits();
      self.poll_session_idle();

      // Try accept one client connection.
      if let Some(stream) = self.try_accept()? {
        self.handle_new_connection(stream)?;
      }

      // Small sleep to avoid busy-spin.
      thread::sleep(Duration::from_millis(50));
    }
    // Best-effort: remove socket on shutdown
    let _ = fs::remove_file(&self.socket_path);
    info!("Daemon shutting down");
    Ok(())
  }

  /// Poll all sessions for exits and broadcast notifications to attached clients.
  fn poll_session_exits(&self) {
    let exited = { self.registry.lock().collect_exited() };
    if !exited.is_empty() {
      // For each exited session, send Exited to its clients and notify subscribers
      for (sid, stats) in exited {
        let chans = self.read_registry(|reg| reg.read_client_controls_for_session(sid));
        let msg = D2CControl::Exited {
          code: None,
          signal: None,
          stats: stats.clone(),
        };
        for ch in chans {
          let _ = ch.send(msg.clone());
        }
        if let Some(pk) = self.read_session_project(sid) {
          self.broadcast_sessions_changed(&pk);
        }
      }
    }
  }

  fn poll_session_idle(&self) {
    let dirty_projects = { self.registry.lock().poll_idle_sessions(Instant::now()) };
    for project in dirty_projects {
      self.broadcast_sessions_changed(&project);
    }
  }

  /// Attempts a non-blocking accept; returns `Ok(None)` on `WouldBlock`.
  fn try_accept(&self) -> anyhow::Result<Option<UnixStream>> {
    match self.listener.accept() {
      Ok((stream, _addr)) => {
        stream.set_nonblocking(false)?;
        Ok(Some(stream))
      }
      Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
      Err(e) => {
        error!("Accept error: {e}");
        thread::sleep(Duration::from_millis(200));
        Ok(None)
      }
    }
  }

  /// Handles a newly accepted connection.
  fn handle_new_connection(&self, mut stream: UnixStream) -> anyhow::Result<()> {
    // Read first frame which can be OpenSession, JoinSession, ListSessions or Shutdown
    match read_frame::<_, C2D>(&mut stream) {
      Ok(C2D::Control(C2DControl::OpenSession { meta, rows, cols })) => {
        self.open_and_attach(stream, &meta, rows, cols)?;
        Ok(())
      }
      Ok(C2D::Control(C2DControl::JoinSession {
        session_id,
        rows,
        cols,
      })) => {
        self.join_and_attach(stream, session_id, rows, cols)?;
        Ok(())
      }
      Ok(C2D::Control(C2DControl::ListSessions { project })) => {
        let entries = self.read_registry(|reg| reg.list_sessions(project.as_ref()));
        let _ = write_frame(&mut stream, &D2C::Control(D2CControl::Sessions { entries }));
        let _ = stream.shutdown(std::net::Shutdown::Both);
        Ok(())
      }
      Ok(C2D::Control(C2DControl::SubscribeEvents { project })) => {
        self.handle_subscribe(stream, project)?;
        Ok(())
      }
      Ok(C2D::Control(C2DControl::NotifyTasksChanged { project })) => {
        self.broadcast_tasks_changed(&project);
        let _ = write_frame(&mut stream, &D2C::Control(D2CControl::Goodbye));
        let _ = stream.shutdown(std::net::Shutdown::Both);
        Ok(())
      }
      Ok(C2D::Control(C2DControl::StopSession { session_id })) => {
        // Determine project for broadcast
        if let Some(pk) = self.read_session_project(session_id) {
          // Stop and collect channels, then send Goodbye outside lock
          let chans = self.write_registry(|reg| reg.stop_session_and_collect_channels(session_id));
          for ch in chans {
            let _ = ch.send(D2CControl::Goodbye);
          }
          self.broadcast_sessions_changed(&pk);
        }
        // Acknowledge with Goodbye for clients expecting it
        let _ = write_frame(&mut stream, &D2C::Control(D2CControl::Goodbye));
        let _ = stream.shutdown(std::net::Shutdown::Both);
        Ok(())
      }
      Ok(C2D::Control(C2DControl::StopTask {
        project,
        task_id,
        slug,
      })) => {
        // Stop all sessions and collect client channels to notify
        let (stopped, chans): (usize, Vec<D2CControlChannel>) = self.write_registry(|reg| {
          let ids: Vec<u64> = reg
            .sessions
            .iter()
            .filter(|(_id, e)| {
              e.meta.project == project && e.meta.task.id == task_id && e.meta.task.slug == slug
            })
            .map(|(id, _)| *id)
            .collect();
          let mut collected = Vec::new();
          for sid in &ids {
            let cs = reg.stop_session_and_collect_channels(*sid);
            collected.extend(cs);
          }
          (ids.len(), collected)
        });
        for ch in chans {
          let _ = ch.send(D2CControl::Goodbye);
        }
        self.broadcast_sessions_changed(&project);
        // Send acknowledgement with number of sessions stopped
        let _ = write_frame(&mut stream, &D2C::Control(D2CControl::Ack { stopped }));
        let _ = stream.shutdown(std::net::Shutdown::Both);
        Ok(())
      }
      Ok(C2D::Control(C2DControl::Shutdown)) => {
        info!("Received Shutdown; stopping daemon loop");
        self
          .shutdown
          .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = stream.shutdown(std::net::Shutdown::Both);
        Ok(())
      }
      Ok(other) => {
        warn!("Unexpected first frame: {other:?}");
        let _ = write_frame(
          &mut stream,
          &D2C::Control(D2CControl::Error {
            message: "First frame must be OpenSession, JoinSession, ListSessions or Shutdown"
              .to_string(),
          }),
        );
        Ok(())
      }
      Err(e) => {
        error!("Handshake read error: {e}");
        Ok(())
      }
    }
  }

  /// Open a new session and attach this connection as a client.
  fn open_and_attach(
    &self,
    stream: UnixStream,
    meta: &SessionOpenMeta,
    rows: u16,
    cols: u16,
  ) -> anyhow::Result<()> {
    // Attempt to find an existing session for the same project/task and reuse it.
    let session_id = {
      let mut reg = self.registry.lock();
      if let Some(existing) =
        reg.find_latest_session_for_task(&meta.project, meta.task.id, &meta.task.slug)
      {
        info!(
          "Reused session {} for project {} task {}",
          existing, &meta.project.repo_root, &meta.task.slug
        );
        let _ = reg.ensure_running_for_attach(existing, rows, cols);
        existing
      } else {
        reg.create_session(meta.clone(), rows, cols)?
      }
    };
    // Broadcast sessions changed for this project
    self.broadcast_sessions_changed(&meta.project);
    self.attach_to_session(stream, session_id, rows, cols)
  }

  /// Attach this connection to an existing session by id.
  fn join_and_attach(
    &self,
    stream: UnixStream,
    session_id: u64,
    rows: u16,
    cols: u16,
  ) -> anyhow::Result<()> {
    // Best-effort: determine project and broadcast clients count changes
    if let Some(pk) = self.read_session_project(session_id) {
      self.broadcast_sessions_changed(&pk);
    }
    self.attach_to_session(stream, session_id, rows, cols)
  }

  /// Attach helper used for both open and join.
  fn attach_to_session(
    &self,
    stream: UnixStream,
    session_id: u64,
    rows: u16,
    cols: u16,
  ) -> anyhow::Result<()> {
    // Channels
    let (control_tx, control_rx) = make_d2c_control_channel();
    let (output_tx, output_rx) = make_output_channel(1024);

    // Register client with session and apply initial size
    let client_id = {
      let mut reg = self.registry.lock();
      reg.apply_resize(session_id, rows, cols);
      reg.attach_client(session_id, control_tx.clone(), output_tx.clone())?
    };

    // Split stream
    let stream_reader = stream;
    let stream_writer = stream_reader.try_clone()?;

    // Send Welcome with snapshot
    if let Some((ansi, (sr, sc))) = self.registry.lock().snapshot(session_id) {
      let _ = control_tx.send_welcome(session_id, sr, sc, ansi);
    }

    // Spawn writer and reader threads
    let writer = Self::spawn_writer_thread(stream_writer, control_rx, output_rx)?;
    let reader = self.spawn_reader_thread(stream_reader, control_tx.clone(), session_id)?;

    // Supervisor to clean up after reader exits
    let registry = self.registry.clone();
    thread::spawn(move || {
      let _ = reader.join();
      {
        let mut reg = registry.lock();
        reg.detach_client(session_id, client_id);
      }
      let _ = writer.join();
      info!("Detached client {client_id} from session {session_id}");
    });

    Ok(())
  }

  /// Spawns the writer thread (Control/Output -> Client) with control priority.
  ///
  /// This thread drains control frames promptly and interleaves output frames,
  /// ensuring detaches and other control messages are delivered even under heavy
  /// output. No daemon locks are held while sending frames.
  fn spawn_writer_thread(
    mut stream_writer: UnixStream,
    control_rx: crossbeam_channel::Receiver<D2CControl>,
    output_rx: crossbeam_channel::Receiver<Vec<u8>>,
  ) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let handle = thread::Builder::new()
      .name("daemon-writer".to_string())
      .spawn(move || {
        let _ = stream_writer.set_write_timeout(Some(Duration::from_secs(1)));
        loop {
          while let Ok(cm) = control_rx.try_recv() {
            if let Err(e) = write_frame(&mut stream_writer, &D2C::Control(cm)) {
              error!("Writer: control frame send error: {e}");
              return;
            }
          }
          crossbeam_channel::select! {
              recv(control_rx) -> msg => {
                  match msg {
                      Ok(cm) => {
                          if let Err(e) = write_frame(&mut stream_writer, &D2C::Control(cm)) {
                              error!("Writer: control send error: {e}");
                              break;
                          }
                      }
                      Err(_) => { break; }
                  }
              }
              recv(output_rx) -> msg => {
                  match msg {
                      Ok(bytes) => {
                          if let Err(e) = write_frame(&mut stream_writer, &D2C::Output { bytes }) {
                              error!("Writer: output send error: {e}");
                              break;
                          }
                      }
                      Err(_) => { break; }
                  }
              }
          }
        }
        info!("Writer thread exiting");
      })?;
    Ok(handle)
  }

  /// Spawns the reader thread (Client -> PTY) that handles C2D frames.
  ///
  /// Reads frames from the client stream and dispatches to the session.
  /// Sends control responses for detach, unexpected attach, and ping.
  fn spawn_reader_thread(
    &self,
    mut stream_reader: UnixStream,
    control_tx: D2CControlChannel,
    session_id: u64,
  ) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let registry_for_reader = self.registry.clone();
    let subscribers_for_reader = self.subscribers.clone();
    let handle = thread::Builder::new()
      .name("daemon-reader".to_string())
      .spawn(move || {
        loop {
          let msg: C2D = match read_frame(&mut stream_reader) {
            Ok(m) => m,
            Err(e) => {
              warn!("Reader: read_frame error or disconnect: {e}");
              break;
            }
          };
          match msg {
            C2D::Input { bytes } => {
              let reg = registry_for_reader.lock();
              reg.write_input(session_id, &bytes);
            }
            C2D::Control(cm) => match cm {
              C2DControl::Resize { rows, cols } => {
                let reg = registry_for_reader.lock();
                reg.apply_resize(session_id, rows, cols);
              }
              C2DControl::Detach => {
                let _ = control_tx.send_goodbye();
                break;
              }
              C2DControl::OpenSession { .. } | C2DControl::JoinSession { .. } => {
                let _ =
                  control_tx.send_error("Unexpected session command after handshake".to_string());
                break;
              }
              C2DControl::Ping { nonce } => {
                let _ = control_tx.send_pong(nonce);
              }
              C2DControl::RestartSession { .. } => {
                // Determine current size without holding the lock for the send
                let (rows_now, cols_now) = registry_for_reader
                  .lock()
                  .snapshot(session_id)
                  .map_or((24, 80), |(_, sz)| sz);

                // Restart the shell with the current size
                let _ = registry_for_reader
                  .lock()
                  .restart_session(session_id, rows_now, cols_now);

                // After restart, send a fresh snapshot to the attached client
                if let Some((ansi, (sr, sc))) = registry_for_reader.lock().snapshot(session_id) {
                  let _ = control_tx.send_welcome(session_id, sr, sc, ansi);
                }

                // Broadcast sessions changed for this session's project so TUI updates status
                let project_opt = {
                  let reg = registry_for_reader.lock();
                  reg
                    .sessions
                    .get(&session_id)
                    .map(|e| e.meta.project.clone())
                };
                if let Some(project) = project_opt {
                  // Build entries and broadcast without holding locks
                  let entries = registry_for_reader.lock().list_sessions(Some(&project));
                  let subs = subscribers_for_reader.lock();
                  for s in subs.iter() {
                    if s.project == project {
                      let _ = s.control.send(D2CControl::SessionsChanged {
                        entries: entries.clone(),
                      });
                    }
                  }
                }
              }
              C2DControl::StopSession { .. } => {
                let _ = registry_for_reader.lock().stop_session(session_id);
                let _ = control_tx.send_goodbye();
                break;
              }
              C2DControl::StopTask { .. }
              | C2DControl::ListSessions { .. }
              | C2DControl::SubscribeEvents { .. }
              | C2DControl::NotifyTasksChanged { .. }
              | C2DControl::Shutdown => {
                let _ = control_tx.send_error("Invalid control in attachment".to_string());
                break;
              }
            },
          }
        }
        info!("Reader thread exiting");
      })?;
    Ok(handle)
  }
}

/// Subscriber entry used to broadcast control events to TUI clients.
pub struct Subscriber {
  pub project: ProjectKey,
  pub control: D2CControlChannel,
}

impl Daemon {
  fn handle_subscribe(&self, stream: UnixStream, project: ProjectKey) -> anyhow::Result<()> {
    let (control_tx, control_rx) = make_d2c_control_channel();
    let mut writer_stream = stream;
    let reader_stream = writer_stream.try_clone()?;

    // Register subscriber
    {
      let mut subs = self.subscribers.lock();
      subs.push(Subscriber {
        project: project.clone(),
        control: control_tx.clone(),
      });
    }

    // Initial sessions snapshot for client convenience
    let initial_entries = self.read_registry(|reg| reg.list_sessions(Some(&project)));
    let _ = control_tx.send(D2CControl::SessionsChanged {
      entries: initial_entries,
    });

    // Writer thread: only control frames
    let writer_handle = std::thread::Builder::new()
      .name("subscriber-writer".to_string())
      .spawn(move || {
        while let Ok(cm) = control_rx.recv() {
          if let Err(e) = write_frame(&mut writer_stream, &D2C::Control(cm)) {
            error!("Subscriber writer error: {e}");
            break;
          }
        }
        info!("Subscriber writer exiting");
      })?;

    // Reader thread: accept ping, ignore others, exit on disconnect
    let subs_list = self.subscribers.clone();
    std::thread::Builder::new()
      .name("subscriber-reader".to_string())
      .spawn(move || {
        loop {
          let msg: C2D = match read_frame(&reader_stream) {
            Ok(m) => m,
            Err(_) => break,
          };
          if let C2D::Control(C2DControl::Ping { nonce }) = msg {
            let _ = control_tx.send(D2CControl::Pong { nonce });
          }
        }
        let _ = writer_handle.join();
        // Remove subscriber entry
        let mut subs = subs_list.lock();
        let idx = subs.iter().position(|s| {
          s.project == project && std::ptr::eq(&raw const s.control, &raw const control_tx)
        });
        if let Some(i) = idx {
          subs.remove(i);
        }
        info!("Subscriber disconnected for project {}", project.repo_root);
      })?;

    Ok(())
  }

  fn broadcast_sessions_changed(&self, project: &ProjectKey) {
    let entries = self.read_registry(|reg| reg.list_sessions(Some(project)));
    let subs = self.subscribers.lock();
    for s in subs.iter() {
      if &s.project == project {
        let _ = s.control.send(D2CControl::SessionsChanged {
          entries: entries.clone(),
        });
      }
    }
  }

  // Removed legacy arcs helper; use instance method `broadcast_sessions_changed` or
  // inline read-then-send pattern in threads where `&self` is not available.

  fn broadcast_tasks_changed(&self, project: &ProjectKey) {
    let subs = self.subscribers.lock();
    for s in subs.iter() {
      if &s.project == project {
        let _ = s.control.send(D2CControl::TasksChanged {
          project: project.clone(),
        });
      }
    }
  }

  // ---- Helpers (locks at bottom for clarity) ----
  fn read_registry<T>(&self, f: impl FnOnce(&SessionRegistry) -> T) -> T {
    let reg = self.registry.lock();
    f(&reg)
  }

  fn write_registry<T>(&self, f: impl FnOnce(&mut SessionRegistry) -> T) -> T {
    let mut reg = self.registry.lock();
    f(&mut reg)
  }

  fn read_session_project(&self, session_id: u64) -> Option<ProjectKey> {
    self.read_registry(|reg| {
      reg
        .sessions
        .get(&session_id)
        .map(|e| e.meta.project.clone())
    })
  }
}

pub fn ensure_socket_dir_and_bind(path: &Path) -> anyhow::Result<UnixListener> {
  if let Some(dir) = path.parent() {
    info!("Ensuring socket directory exists: {}", dir.display());
    fs::create_dir_all(dir).with_context(|| format!("create dir {}", dir.display()))?;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    debug!("Set permissions to 0700 for {}", dir.display());
  }

  if path.exists() {
    warn!("Socket path already exists: {}", path.display());
    match UnixStream::connect(path) {
      Ok(_) => {
        warn!("Existing socket is live; daemon already running");
        anyhow::bail!("daemon already running")
      }
      Err(e) => {
        info!(
          "Stale socket detected (connect failed: {}); removing {}",
          e,
          path.display()
        );
        let _ = fs::remove_file(path);
      }
    }
  }

  info!("Binding Unix listener at {}", path.display());
  let listener = UnixListener::bind(path)
    .map_err(|e| anyhow::anyhow!("bind unix listener at {}: {}", path.display(), e))?;
  Ok(listener)
}
