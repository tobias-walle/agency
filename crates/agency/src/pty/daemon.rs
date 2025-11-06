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
  C2D, C2DControl, D2C, D2CControl, D2CControlChannel, D2COutputChannel, make_d2c_control_channel,
  make_output_channel, read_frame, write_frame,
};
use crate::pty::session::Session;
use crate::utils::command::Command;
use anyhow::Context;
use log::{debug, error, info, warn};
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn run_daemon(socket_path: &Path, cmd: Command) -> anyhow::Result<()> {
  info!("Starting daemon. Socket path: {}", socket_path.display());
  // If another daemon is already running, bail early by attempting a connect.
  if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
    warn!("Daemon is already running");
    return Ok(());
  }

  let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
  let daemon = Daemon::new(socket_path, rows, cols, cmd)?;
  daemon.run()
}

/// Represents the daemon's attachment lifecycle and state management.
///
/// The daemon only allows a single client attachment at a time.
/// When attached, it owns the control and output channels as well as
/// the reader/writer thread handles.
pub enum DaemonState {
  /// No client is attached; the daemon is ready to accept a new client.
  Idle,
  /// A client is attached with the given resources.
  Attached(Attachment),
}

/// Holds resources for a single attached client.
///
/// Dropping this struct closes its channels, signaling shutdown to the writer.
pub struct Attachment {
  /// Control channel used for reliable low-volume frames.
  pub control: D2CControlChannel,
  /// Lossy output channel kept alive while attached.
  pub _output: D2COutputChannel,
  /// Join handle for the reader thread (Client -> PTY).
  pub reader: Option<std::thread::JoinHandle<()>>,
  /// Join handle for the writer thread (Control/Output -> Client).
  pub writer: Option<std::thread::JoinHandle<()>>,
}

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
  /// Shared `Session` running the shell and bridging PTY IO.
  session: Arc<Mutex<Session>>,
  /// Current daemon state (Idle or Attached).
  state: Arc<Mutex<DaemonState>>,
  /// Path to the bound Unix socket for cleanup.
  socket_path: PathBuf,
  /// Shutdown flag set when a `Shutdown` control is received.
  shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl Daemon {
  /// Constructs and configures a new `Daemon`.
  ///
  /// Binds the socket at `socket_path`, creates a `Session` with initial
  /// rows/cols, and sets the listener to non-blocking.
  pub fn new(socket_path: &Path, rows: u16, cols: u16, cmd: Command) -> anyhow::Result<Self> {
    let listener = ensure_socket_dir_and_bind(socket_path)?;
    listener.set_nonblocking(true)?;
    let session = Arc::new(Mutex::new(Session::new(rows, cols, cmd)?));
    Ok(Self {
      listener,
      session,
      state: Arc::new(Mutex::new(DaemonState::Idle)),
      socket_path: socket_path.to_path_buf(),
      shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })
  }

  /// IMPORTANT: Never hold `session` or `state` locks while sending frames.
  /// These helpers scope `Mutex` guards to the closure and return owned data.
  /// Do not perform channel sends inside the closure; extract data and drop
  /// the guard first.
  pub fn with_session_ref<R>(&self, f: impl FnOnce(&Session) -> R) -> R {
    let guard = self.session.lock().unwrap();
    f(&guard)
  }

  /// See `with_session_ref`. This variant allows mutable access for operations
  /// that require `&mut Session` such as `try_wait_child` or `restart_shell`.
  pub fn with_session_mut<R>(&self, f: impl FnOnce(&mut Session) -> R) -> R {
    let mut guard = self.session.lock().unwrap();
    f(&mut guard)
  }

  /// Immutable access to daemon state within a short lock scope.
  pub fn with_state_ref<R>(&self, f: impl FnOnce(&DaemonState) -> R) -> R {
    let guard = self.state.lock().unwrap();
    f(&guard)
  }

  /// Mutable access to daemon state within a short lock scope.
  pub fn with_state_mut<R>(&self, f: impl FnOnce(&mut DaemonState) -> R) -> R {
    let mut guard = self.state.lock().unwrap();
    f(&mut guard)
  }

  /// Returns a cloned control channel if a client is attached.
  /// No locks are held during any subsequent send operations.
  pub fn attached_control_channel(&self) -> Option<D2CControlChannel> {
    self.with_state_ref(|st| match st {
      DaemonState::Attached(att) => Some(att.control.clone()),
      DaemonState::Idle => None,
    })
  }

  /// Returns true if a client is currently attached.
  pub fn is_attached(&self) -> bool {
    self.with_state_ref(|st| matches!(st, DaemonState::Attached(_)))
  }

  /// Runs the main accept loop interleaving child exit checks with accepts.
  pub fn run(&self) -> anyhow::Result<()> {
    info!("Daemon running");
    while !self.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
      // Handle child exit and possible restart + notifications.
      self.poll_child_exit()?;

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

  /// Checks whether the shell child exited, and if so, performs restart.
  /// If attached, sends `Exited` and a fresh `Hello` + `Snapshot`.
  fn poll_child_exit(&self) -> anyhow::Result<()> {
    let exited = self.with_session_mut(|s| s.try_wait_child());
    if let Some(status) = exited {
      info!("Child process exited: {:?}", status);
      // Get size and stats without generating ANSI unnecessarily
      let (rows, cols) = self.with_session_ref(|s| s.size());
      let stats = self.with_session_ref(|s| s.stats_lite());

      // Clone control channel if attached without holding lock during sends
      let control_opt = self.attached_control_channel();

      if let Some(control) = control_opt {
        let _ = control.send_exited(None, None, stats);
        let _ = self.with_session_mut(|s| s.restart_shell(rows, cols));
        let (ansi, (sr2, sc2)) = self.with_session_ref(|s| s.snapshot());
        let _ = control.send_hello(sr2, sc2);
        let _ = control.send_snapshot(ansi, sr2, sc2);
      } else {
        let _ = self.with_session_mut(|s| s.restart_shell(rows, cols));
      }
    }
    Ok(())
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
        error!("Accept error: {}", e);
        thread::sleep(Duration::from_millis(200));
        Ok(None)
      }
    }
  }

  /// Handles a newly accepted connection, performing busy-check and handshake,
  /// then attaching the client if valid.
  fn handle_new_connection(&self, mut stream: UnixStream) -> anyhow::Result<()> {
    // Read first frame which can be Attach or Shutdown
    match read_frame::<_, C2D>(&mut stream) {
      Ok(C2D::Control(C2DControl::Attach {
        rows,
        cols,
        task: _,
        ..
      })) => {
        if self.is_attached() {
          warn!("Client attempted attach while another is already attached");
          self.reject_busy(&mut stream)?;
          return Ok(());
        }
        self.attach_client(stream, rows, cols)?;
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
        warn!("Unexpected first frame: {:?}", other);
        let _ = write_frame(
          &mut stream,
          &D2C::Control(D2CControl::Error {
            message: "First frame must be Attach or Shutdown".to_string(),
          }),
        );
        Ok(())
      }
      Err(e) => {
        error!("Handshake read error: {}", e);
        Ok(())
      }
    }
  }

  /// Sends a busy error and closes the stream.
  fn reject_busy(&self, stream: &mut UnixStream) -> anyhow::Result<()> {
    let _ = write_frame(
      &mut *stream,
      &D2C::Control(D2CControl::Error {
        message: "Already attached".to_string(),
      }),
    );
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(())
  }

  /// Attaches the client: applies resize, configures channels, spawns threads,
  /// sends initial `Hello` + `Snapshot`, and supervises lifecycle.
  fn attach_client(&self, stream: UnixStream, rows: u16, cols: u16) -> anyhow::Result<()> {
    self.with_session_ref(|s| s.apply_resize(rows, cols));
    let (ansi, (sr, sc)) = self.with_session_ref(|s| s.snapshot());

    // Channels
    let (control_tx, control_rx) = make_d2c_control_channel();
    let (output_tx, output_rx) = make_output_channel(1024);

    // Split stream
    let stream_reader = stream;
    let stream_writer = stream_reader.try_clone()?;

    // Writer thread with control priority
    let writer = self.spawn_writer_thread(stream_writer, control_rx, output_rx)?;

    // Configure session output sink
    self.with_session_ref(|s| s.set_output_sink(Some(output_tx.clone())));

    // Reader thread
    let reader = self.spawn_reader_thread(stream_reader, control_tx.clone())?;

    // Save attachment in state
    self.with_state_mut(|g| {
      *g = DaemonState::Attached(Attachment {
        control: control_tx.clone(),
        _output: output_tx.clone(),
        reader: Some(reader),
        writer: Some(writer),
      });
    });

    // Initial Hello + Snapshot via control
    let _ = control_tx.send_hello(sr, sc);
    let _ = control_tx.send_snapshot(ansi, sr, sc);

    // Supervisor to cleanup and allow new attachments
    let _ = self.spawn_supervisor_thread()?;

    Ok(())
  }

  /// Spawns the writer thread (Control/Output -> Client) with control priority.
  ///
  /// This thread drains control frames promptly and interleaves output frames,
  /// ensuring detaches and other control messages are delivered even under heavy
  /// output. No daemon locks are held while sending frames.
  fn spawn_writer_thread(
    &self,
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
              error!("Writer: control frame send error: {}", e);
              return;
            }
          }
          crossbeam_channel::select! {
              recv(control_rx) -> msg => {
                  match msg {
                      Ok(cm) => {
                          if let Err(e) = write_frame(&mut stream_writer, &D2C::Control(cm)) {
                              error!("Writer: control send error: {}", e);
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
                              error!("Writer: output send error: {}", e);
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
  ) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let session_for_reader = self.session.clone();
    let handle = thread::Builder::new()
      .name("daemon-reader".to_string())
      .spawn(move || {
        loop {
          let msg: C2D = match read_frame(&mut stream_reader) {
            Ok(m) => m,
            Err(e) => {
              warn!("Reader: read_frame error or disconnect: {}", e);
              break;
            }
          };
          match msg {
            C2D::Input { bytes } => {
              let _ = session_for_reader.lock().unwrap().write_input(&bytes);
            }
            C2D::Control(cm) => match cm {
              C2DControl::Resize { rows, cols } => {
                session_for_reader.lock().unwrap().apply_resize(rows, cols);
              }
              C2DControl::Detach => {
                let _ = control_tx.send_goodbye();
                break;
              }
              C2DControl::Attach { .. } => {
                let _ = control_tx.send_error("Unexpected Attach after handshake".to_string());
                break;
              }
              C2DControl::Ping { nonce } => {
                let _ = control_tx.send_pong(nonce);
              }
              C2DControl::Shutdown => {
                // Treat as a detach at the connection level
                let _ = control_tx.send_goodbye();
                break;
              }
            },
          }
        }
        info!("Reader thread exiting");
      })?;
    Ok(handle)
  }

  /// Spawns the supervisor thread responsible for cleanup and allowing new attachments.
  ///
  /// Joins the reader, clears the output sink, takes and asynchronously joins the
  /// writer, and flips the daemon state back to `Idle`.
  fn spawn_supervisor_thread(&self) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let session_for_supervisor = self.session.clone();
    let state_for_supervisor = self.state.clone();
    let handle = thread::Builder::new()
      .name("daemon-supervisor".to_string())
      .spawn(move || {
        // Join reader first
        let reader_handle_opt = {
          let mut g = state_for_supervisor.lock().unwrap();
          match &mut *g {
            DaemonState::Attached(att) => att.reader.take(),
            _ => None,
          }
        };
        if let Some(rh) = reader_handle_opt {
          let _ = rh.join();
        }
        // Clear output sink
        session_for_supervisor.lock().unwrap().set_output_sink(None);
        // Take writer and drop attachment to close channels
        let writer_handle_opt = {
          let mut g = state_for_supervisor.lock().unwrap();
          match &mut *g {
            DaemonState::Attached(att) => att.writer.take(),
            _ => None,
          }
        };
        {
          let mut g = state_for_supervisor.lock().unwrap();
          *g = DaemonState::Idle;
        }
        // Join writer asynchronously
        if let Some(wh) = writer_handle_opt {
          thread::spawn(move || {
            let _ = wh.join();
            info!("Writer thread joined");
          });
        }
        info!("Attachment cleared; ready for new clients");
      })?;
    Ok(handle)
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
    .with_context(|| format!("bind unix listener at {}", path.display()))?;
  Ok(listener)
}
