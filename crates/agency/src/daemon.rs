use crate::daemon_protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionInfo, TaskInfo, TaskMeta, TaskMetrics,
  read_frame, write_frame,
};
use crate::utils::git::{
  commits_ahead_at, current_branch_name_at, git_workdir, uncommitted_numstat_at,
};
use crate::utils::status::is_task_completed;
use crate::utils::task::{TaskRef, branch_name, list_tasks, read_task_frontmatter, worktree_dir};
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
  last_snapshot: Arc<Mutex<HashMap<String, ProjectSnapshot>>>,
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

  fn snapshot_for(&self, project: &ProjectKey) -> anyhow::Result<ProjectSnapshot> {
    let prev = self.last_snapshot.lock().get(&project.repo_root).cloned();
    build_project_snapshot(&self.cfg, project, prev.as_ref())
  }

  fn broadcast_state(&self, project: &ProjectKey, snap: &ProjectSnapshot) {
    let mut remove_idx = Vec::new();
    let mut subs = self.subscribers.lock();
    for (i, sub) in subs.iter_mut().enumerate() {
      if sub.project.repo_root == project.repo_root
        && write_frame(
          &mut sub.stream,
          &D2C::Control(D2CControl::ProjectState {
            project: project.clone(),
            tasks: snap.tasks.clone(),
            sessions: snap.sessions.clone(),
            metrics: snap.metrics.clone(),
          }),
        )
        .is_err()
      {
        remove_idx.push(i);
      }
    }
    for i in remove_idx.into_iter().rev() {
      subs.remove(i);
    }
  }

  fn update_cache_and_broadcast(&self, project: &ProjectKey, snap: ProjectSnapshot) {
    self
      .last_snapshot
      .lock()
      .insert(project.repo_root.clone(), snap.clone());
    self.broadcast_state(project, &snap);
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
          std::thread::sleep(Duration::from_millis(1000));
          let targets: Vec<ProjectKey> = subs.lock().iter().map(|s| s.project.clone()).collect();
          for pk in targets {
            let prev = cache.lock().get(&pk.repo_root).cloned();
            match build_project_snapshot(&cfg, &pk, prev.as_ref()) {
              Ok(new_snap) => {
                let mut cache_guard = cache.lock();
                let changed = cache_guard.get(&pk.repo_root) != Some(&new_snap);
                if changed {
                  cache_guard.insert(pk.repo_root.clone(), new_snap.clone());
                  broadcast_project_state(&subs, &pk, &new_snap);
                }
              }
              Err(_) => {
                // Ignore snapshot errors in poller; clients may retry
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
      Ok(C2D::Control(C2DControl::ListProjectState { project })) => {
        match self.snapshot_for(&project) {
          Ok(new_snap) => {
            let _ = write_frame(
              &mut *stream,
              &D2C::Control(D2CControl::ProjectState {
                project,
                tasks: new_snap.tasks,
                sessions: new_snap.sessions,
                metrics: new_snap.metrics,
              }),
            );
          }
          Err(err) => {
            let _ = write_frame(
              &mut *stream,
              &D2C::Control(D2CControl::Error {
                message: format!("Snapshot error: {err}"),
              }),
            );
          }
        }
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
        if let Ok(snap) = self.snapshot_for(&project) {
          let _ = write_frame(
            &mut *stream,
            &D2C::Control(D2CControl::ProjectState {
              project: project.clone(),
              tasks: snap.tasks.clone(),
              sessions: snap.sessions.clone(),
              metrics: snap.metrics.clone(),
            }),
          );
        }
        // Store subscriber; keep stream open until disconnect
        let cloned = stream.try_clone()?;
        self.subscribers.lock().push(Subscriber {
          project,
          stream: cloned,
        });
      }
      Ok(C2D::Control(C2DControl::NotifyTasksChanged { project })) => {
        // Recompute and broadcast a fresh snapshot for the project
        if let Ok(snap) = self.snapshot_for(&project) {
          self.update_cache_and_broadcast(&project, snap);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectSnapshot {
  tasks: Vec<TaskInfo>,
  sessions: Vec<SessionInfo>,
  metrics: Vec<TaskMetrics>,
}

fn now_ms() -> u64 {
  use std::time::{SystemTime, UNIX_EPOCH};
  let dur = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap_or_else(|_| Duration::from_secs(0));
  dur.as_millis() as u64
}

fn build_project_snapshot(
  cfg: &crate::config::AgencyConfig,
  project: &ProjectKey,
  prev: Option<&ProjectSnapshot>,
) -> anyhow::Result<ProjectSnapshot> {
  let root = Path::new(&project.repo_root);
  // Sessions from tmux
  let sessions = tmux_list(cfg, root).unwrap_or_default();

  // Task index
  let paths = crate::config::AgencyPaths::new(root);
  let task_refs = list_tasks(&paths).unwrap_or_default();

  // Determine base branch default from repo HEAD
  let repo_root = git_workdir(root).unwrap_or_else(|_| root.to_path_buf());
  let head = current_branch_name_at(&repo_root)
    .unwrap_or(Some("main".to_string()))
    .unwrap_or("main".to_string());

  let mut tasks_info: Vec<TaskInfo> = Vec::new();
  for tref in &task_refs {
    let fm = read_task_frontmatter(&paths, tref);
    let base = fm
      .and_then(|f| f.base_branch)
      .unwrap_or_else(|| head.clone());
    tasks_info.push(TaskInfo {
      id: tref.id,
      slug: tref.slug.clone(),
      base_branch: base,
    });
  }

  // Determine candidate tasks: Running from sessions or Completed via flags
  use std::collections::HashSet;
  let running: HashSet<(u32, String)> = sessions
    .iter()
    .filter(|s| s.status == "Running" || s.status == "Idle" || s.status == "Exited")
    .map(|s| (s.task.id, s.task.slug.clone()))
    .collect();

  let mut candidates: HashSet<(u32, String)> = running.clone();
  for t in &task_refs {
    if is_task_completed(&paths, t) {
      candidates.insert((t.id, t.slug.clone()));
    }
  }

  // Compute metrics
  let mut metrics: Vec<TaskMetrics> = Vec::new();
  for (id, slug) in candidates {
    let tref = TaskRef {
      id,
      slug: slug.clone(),
    };
    let task = TaskMeta {
      id,
      slug: slug.clone(),
    };
    let wt = worktree_dir(&paths, &tref);
    let (add, del) = if wt.exists() {
      uncommitted_numstat_at(&wt).unwrap_or((0, 0))
    } else {
      (0, 0)
    };
    // Resolve base for this task
    let base = tasks_info
      .iter()
      .find(|ti| ti.id == id && ti.slug == slug)
      .map_or_else(|| head.clone(), |ti| ti.base_branch.clone());
    let branch = branch_name(&tref);
    let ahead = commits_ahead_at(&repo_root, &base, &branch).unwrap_or(0);
    let mut updated_at_ms = now_ms();
    if let Some(prev_snap) = prev
      && let Some(prev_m) = prev_snap
        .metrics
        .iter()
        .find(|m| m.task.id == id && m.task.slug == slug)
      && prev_m.uncommitted_add == add
      && prev_m.uncommitted_del == del
      && prev_m.commits_ahead == ahead
    {
      updated_at_ms = prev_m.updated_at_ms;
    }
    metrics.push(TaskMetrics {
      task,
      uncommitted_add: add,
      uncommitted_del: del,
      commits_ahead: ahead,
      updated_at_ms,
    });
  }

  // Sort for stable equality
  tasks_info.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.slug.cmp(&b.slug)));
  let mut sessions_sorted = sessions.clone();
  sessions_sorted.sort_by(|a, b| a.session_id.cmp(&b.session_id));
  metrics.sort_by(|a, b| {
    a.task
      .id
      .cmp(&b.task.id)
      .then_with(|| a.task.slug.cmp(&b.task.slug))
  });

  Ok(ProjectSnapshot {
    tasks: tasks_info,
    sessions: sessions_sorted,
    metrics,
  })
}

// Helper for the poller: broadcast snapshot to all subscribers of a project.
fn broadcast_project_state(
  subs: &Arc<Mutex<Vec<Subscriber>>>,
  project: &ProjectKey,
  snap: &ProjectSnapshot,
) {
  let mut remove_idx = Vec::new();
  let mut subs_list = subs.lock();
  for (i, sub) in subs_list.iter_mut().enumerate() {
    if sub.project.repo_root == project.repo_root
      && write_frame(
        &mut sub.stream,
        &D2C::Control(D2CControl::ProjectState {
          project: project.clone(),
          tasks: snap.tasks.clone(),
          sessions: snap.sessions.clone(),
          metrics: snap.metrics.clone(),
        }),
      )
      .is_err()
    {
      remove_idx.push(i);
    }
  }
  for i in remove_idx.into_iter().rev() {
    subs_list.remove(i);
  }
}
