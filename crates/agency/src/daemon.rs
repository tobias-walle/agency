use crate::daemon_protocol as proto;
use crate::daemon_protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, read_frame, write_frame,
};
use crate::utils::tmux::list_sessions_for_project as tmux_list;
use anyhow::Result;
use log::{error, info, warn};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub fn run_daemon(socket_path: &Path, cfg: &crate::config::AgencyConfig) -> Result<()> {
  info!("Starting daemon. Socket path: {}", socket_path.display());
  if std::os::unix::net::UnixStream::connect(socket_path).is_ok() {
    warn!("Daemon is already running");
    return Ok(());
  }

  let listener = ensure_socket_dir_and_bind(socket_path)?;
  listener.set_nonblocking(true)?;
  let daemon = SlimDaemon::new(listener, cfg.clone(), socket_path.to_path_buf());
  daemon.run()
}

pub struct SlimDaemon {
  listener: UnixListener,
  cfg: crate::config::AgencyConfig,
  shutdown: Arc<std::sync::atomic::AtomicBool>,
  subscribers: Arc<Mutex<Vec<Subscriber>>>,
  // Cache last snapshot per project to avoid redundant broadcasts
  last_snapshot: Arc<Mutex<HashMap<String, Vec<proto::SessionInfo>>>>,
  socket_path: PathBuf,
}

struct Subscriber {
  project: ProjectKey,
  stream: UnixStream,
}

impl SlimDaemon {
  #[must_use]
  pub fn new(
    listener: UnixListener,
    cfg: crate::config::AgencyConfig,
    socket_path: PathBuf,
  ) -> Self {
    Self {
      listener,
      cfg,
      shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
      subscribers: Arc::new(Mutex::new(Vec::new())),
      last_snapshot: Arc::new(Mutex::new(HashMap::new())),
      socket_path,
    }
  }

  pub fn run(&self) -> Result<()> {
    // Poller thread to refresh sessions for all subscribed projects
    let subs = self.subscribers.clone();
    let cfg = self.cfg.clone();
    let cache = self.last_snapshot.clone();
    std::thread::Builder::new()
      .name("daemon-poller".to_string())
      .spawn(move || {
        loop {
          std::thread::sleep(Duration::from_millis(250));
          let targets: Vec<ProjectKey> = subs.lock().iter().map(|s| s.project.clone()).collect();
          for pk in targets {
            let path = std::path::PathBuf::from(&pk.repo_root);
            let snap = tmux_list(&cfg, &path).unwrap_or_default();
            let mut last = cache.lock();
            let prev = last.get(&pk.repo_root);
            let changed = prev != Some(&snap);
            if changed {
              last.insert(pk.repo_root.clone(), snap.clone());
              // Broadcast
              let mut remove_idx = Vec::new();
              let mut subs_list = subs.lock();
              for (i, sub) in subs_list.iter_mut().enumerate() {
                if sub.project.repo_root == pk.repo_root
                  && write_frame(
                    &mut sub.stream,
                    &D2C::Control(D2CControl::SessionsChanged {
                      entries: snap.clone(),
                    }),
                  )
                  .is_err()
                {
                  remove_idx.push(i);
                }
              }
              // Remove disconnected subscribers
              for i in remove_idx.into_iter().rev() {
                subs_list.remove(i);
              }
            }
          }
        }
      })?;

    while !self.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
      match self.listener.accept() {
        Ok((mut stream, _)) => {
          if let Err(err) = self.handle_connection(&mut stream) {
            error!("Connection error: {err}");
          }
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
          std::thread::sleep(Duration::from_millis(50));
        }
        Err(e) => {
          error!("Accept error: {e}");
          std::thread::sleep(Duration::from_millis(100));
        }
      }
    }

    // Cleanup socket on shutdown
    let _ = fs::remove_file(&self.socket_path);
    Ok(())
  }

  fn handle_connection(&self, stream: &mut UnixStream) -> Result<()> {
    let first = read_frame::<_, C2D>(&mut *stream);
    match first {
      Ok(C2D::Control(C2DControl::ListSessions { project })) => {
        let entries = if let Some(pk) = project {
          tmux_list(&self.cfg, Path::new(&pk.repo_root)).unwrap_or_default()
        } else {
          Vec::new()
        };
        let _ = write_frame(
          &mut *stream,
          &D2C::Control(D2CControl::Sessions { entries }),
        );
      }
      Ok(C2D::Control(C2DControl::GetVersion)) => {
        let ver = crate::utils::version::get_version().to_string();
        let _ = write_frame(
          &mut *stream,
          &D2C::Control(D2CControl::Version { version: ver }),
        );
      }
      Ok(C2D::Control(C2DControl::SubscribeEvents { project })) => {
        // Send initial snapshot
        let entries = tmux_list(&self.cfg, Path::new(&project.repo_root)).unwrap_or_default();
        let _ = write_frame(
          &mut *stream,
          &D2C::Control(D2CControl::SessionsChanged { entries }),
        );
        // Store subscriber; keep stream open until disconnect
        let cloned = stream.try_clone()?;
        self.subscribers.lock().push(Subscriber {
          project,
          stream: cloned,
        });
      }
      Ok(C2D::Control(C2DControl::NotifyTasksChanged { project })) => {
        // Broadcast both TasksChanged and SessionsChanged for convenience
        let entries = tmux_list(&self.cfg, Path::new(&project.repo_root)).unwrap_or_default();
        let mut remove_idx = Vec::new();
        let mut subs = self.subscribers.lock();
        for (i, sub) in subs.iter_mut().enumerate() {
          if sub.project.repo_root == project.repo_root {
            let _ = write_frame(
              &mut sub.stream,
              &D2C::Control(D2CControl::TasksChanged {
                project: project.clone(),
              }),
            );
            if write_frame(
              &mut sub.stream,
              &D2C::Control(D2CControl::SessionsChanged {
                entries: entries.clone(),
              }),
            )
            .is_err()
            {
              remove_idx.push(i);
            }
          }
        }
        for i in remove_idx.into_iter().rev() {
          subs.remove(i);
        }
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Ack { stopped: 0 }));
      }
      Ok(C2D::Control(C2DControl::StopSession { session_id })) => {
        // Find session by id in all projects with subscribers first; fallback to best-effort kill
        let all_projects: Vec<ProjectKey> = self
          .subscribers
          .lock()
          .iter()
          .map(|s| s.project.clone())
          .collect();
        let mut stopped = 0usize;
        for pk in all_projects {
          let list = tmux_list(&self.cfg, Path::new(&pk.repo_root)).unwrap_or_default();
          if let Some(si) = list.iter().find(|s| s.session_id == session_id) {
            let _ = crate::utils::tmux::kill_session(&self.cfg, &si.task);
            stopped = 1;
            break;
          }
        }
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Ack { stopped }));
      }
      Ok(C2D::Control(C2DControl::StopTask {
        project,
        task_id,
        slug,
      })) => {
        let list = tmux_list(&self.cfg, Path::new(&project.repo_root)).unwrap_or_default();
        let mut stopped = 0usize;
        for si in list {
          if si.task.id == task_id && si.task.slug == slug {
            let _ = crate::utils::tmux::kill_session(&self.cfg, &si.task);
            stopped += 1;
          }
        }
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Ack { stopped }));
      }
      Ok(C2D::Control(C2DControl::Shutdown)) => {
        self
          .shutdown
          .store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Goodbye));
      }
      Ok(C2D::Control(C2DControl::Ping { nonce })) => {
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Pong { nonce }));
      }

      Err(err) => {
        let _ = write_frame(
          &mut *stream,
          &D2C::Control(D2CControl::Error {
            message: format!("Read error: {err}"),
          }),
        );
      }
    }
    Ok(())
  }
}

pub fn ensure_socket_dir_and_bind(path: &Path) -> anyhow::Result<UnixListener> {
  if let Some(dir) = path.parent() {
    let _ = fs::create_dir_all(dir);
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
  }
  if path.exists() {
    // Best-effort remove stale
    let _ = fs::remove_file(path);
  }
  let listener = UnixListener::bind(path)?;
  Ok(listener)
}
