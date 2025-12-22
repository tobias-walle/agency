use crate::config::AgencyConfig;
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

pub fn run_daemon(socket_path: &Path, cfg: &AgencyConfig) -> Result<()> {
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
  cfg: AgencyConfig,
  shutdown: Arc<std::sync::atomic::AtomicBool>,
  subscribers: Arc<Mutex<Vec<Subscriber>>>,
  // Cache last snapshot per project to avoid redundant broadcasts
  last_snapshot: Arc<Mutex<HashMap<String, ProjectSnapshot>>>,
  socket_path: PathBuf,
  // Per-project TUI registry: id -> entry
  tui_registry: Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>,
}

struct Subscriber {
  project: ProjectKey,
  stream: UnixStream,
}

impl SlimDaemon {
  #[must_use]
  pub fn new(listener: UnixListener, cfg: AgencyConfig, socket_path: PathBuf) -> Self {
    Self {
      listener,
      cfg,
      shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
      subscribers: Arc::new(Mutex::new(Vec::new())),
      last_snapshot: Arc::new(Mutex::new(HashMap::new())),
      socket_path,
      tui_registry: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  fn snapshot_for(&self, project: &ProjectKey) -> ProjectSnapshot {
    let prev = self.last_snapshot.lock().get(&project.repo_root).cloned();
    build_project_snapshot(&self.cfg, project, prev.as_ref())
  }

  fn update_cache_and_broadcast(&self, project: &ProjectKey, snap: &ProjectSnapshot) {
    self
      .last_snapshot
      .lock()
      .insert(project.repo_root.clone(), snap.clone());
    broadcast_project_state(&self.subscribers, project, snap);
  }

  pub fn run(&self) -> Result<()> {
    // Poller thread to refresh sessions for all subscribed projects
    let subs = self.subscribers.clone();
    let cfg = self.cfg.clone();
    let cache = self.last_snapshot.clone();
    let registry = self.tui_registry.clone();
    std::thread::Builder::new()
      .name("daemon-poller".to_string())
      .spawn(move || {
        let mut counter: u32 = 0;
        loop {
          std::thread::sleep(Duration::from_millis(1000));
          let targets: Vec<ProjectKey> = subs.lock().iter().map(|s| s.project.clone()).collect();
          for pk in targets {
            let prev = cache.lock().get(&pk.repo_root).cloned();
            let new_snap = build_project_snapshot(&cfg, &pk, prev.as_ref());
            let mut cache_guard = cache.lock();
            let changed = cache_guard.get(&pk.repo_root) != Some(&new_snap);
            if changed {
              cache_guard.insert(pk.repo_root.clone(), new_snap.clone());
              broadcast_project_state(&subs, &pk, &new_snap);
            }
          }
          // Liveness: best-effort every ~10s
          counter = counter.wrapping_add(1);
          if counter % 10 == 0 {
            prune_dead_tuis(&registry);
          }
        }
      })?;

    while !self.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
      match self.listener.accept() {
        Ok((mut stream, _)) => {
          self.handle_connection(&mut stream);
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

  fn handle_connection(&self, stream: &mut UnixStream) {
    let first = read_frame::<_, C2D>(&mut *stream);
    match first {
      Ok(C2D::Control(C2DControl::ListProjectState { project })) => {
        self.write_project_state(stream, &project);
      }
      Ok(C2D::Control(C2DControl::GetVersion)) => {
        Self::write_version(stream);
      }
      Ok(C2D::Control(C2DControl::SubscribeEvents { project })) => {
        self.handle_subscribe(stream, &project);
      }
      Ok(C2D::Control(C2DControl::TuiRegister { project, pid })) => {
        self.handle_tui_register(stream, &project, pid);
      }
      Ok(C2D::Control(C2DControl::TuiUnregister { project, pid })) => {
        unregister_tui(&self.tui_registry, &project.repo_root, pid);
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Ack { stopped: 0 }));
      }
      Ok(C2D::Control(C2DControl::TuiList { project })) => {
        let items = list_tuis(&self.tui_registry, &project.repo_root);
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::TuiList { items }));
      }
      Ok(C2D::Control(C2DControl::TuiFollow { project, tui_id })) => {
        self.handle_tui_follow(stream, &project, tui_id);
      }
      Ok(C2D::Control(C2DControl::TuiFocusTaskChange {
        project,
        tui_id,
        task_id,
      })) => {
        update_tui_focus(&self.tui_registry, &project.repo_root, tui_id, task_id);
        broadcast_tui_focus(&self.subscribers, &project, tui_id, task_id);
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Ack { stopped: 0 }));
      }
      Ok(C2D::Control(C2DControl::NotifyTasksChanged { project })) => {
        let snap = self.snapshot_for(&project);
        self.update_cache_and_broadcast(&project, &snap);
        let _ = write_frame(&mut *stream, &D2C::Control(D2CControl::Ack { stopped: 0 }));
      }
      Ok(C2D::Control(C2DControl::StopSession { session_id })) => {
        self.handle_stop_session(stream, session_id);
      }
      Ok(C2D::Control(C2DControl::StopTask {
        project,
        task_id,
        slug,
      })) => {
        self.handle_stop_task(stream, &project, task_id, &slug);
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
          stream,
          &D2C::Control(D2CControl::Error {
            message: format!("Read error: {err}"),
          }),
        );
      }
    }
  }

  fn write_project_state(&self, stream: &mut UnixStream, project: &ProjectKey) {
    let new_snap = self.snapshot_for(project);
    let _ = write_frame(
      &mut *stream,
      &D2C::Control(D2CControl::ProjectState {
        project: project.clone(),
        tasks: new_snap.tasks,
        sessions: new_snap.sessions,
        metrics: new_snap.metrics,
      }),
    );
  }

  fn write_version(stream: &mut UnixStream) {
    let ver = crate::utils::version::get_version().to_string();
    let _ = write_frame(
      &mut *stream,
      &D2C::Control(D2CControl::Version { version: ver }),
    );
  }

  fn handle_subscribe(&self, stream: &mut UnixStream, project: &ProjectKey) {
    let snap = self.snapshot_for(project);
    let _ = write_frame(
      &mut *stream,
      &D2C::Control(D2CControl::ProjectState {
        project: project.clone(),
        tasks: snap.tasks.clone(),
        sessions: snap.sessions.clone(),
        metrics: snap.metrics.clone(),
      }),
    );
    let cloned = stream.try_clone().unwrap();
    self.subscribers.lock().push(Subscriber {
      project: project.clone(),
      stream: cloned,
    });
  }

  fn handle_tui_register(&self, stream: &mut UnixStream, project: &ProjectKey, pid: u32) {
    let tui_id = assign_tui_id(&self.tui_registry, &project.repo_root, pid);
    let _ = write_frame(
      &mut *stream,
      &D2C::Control(D2CControl::TuiRegistered { tui_id }),
    );
  }

  fn handle_tui_follow(&self, stream: &mut UnixStream, project: &ProjectKey, tui_id: u32) {
    if let Some(entry) = get_tui(&self.tui_registry, &project.repo_root, tui_id) {
      let _ = write_frame(
        &mut *stream,
        &D2C::Control(D2CControl::TuiFollowSucceeded { tui_id }),
      );
      if let Some(task_id) = entry.focused_task_id {
        let _ = write_frame(
          &mut *stream,
          &D2C::Control(D2CControl::TuiFocusTaskChanged {
            project: project.clone(),
            tui_id,
            task_id,
          }),
        );
      }
    } else {
      let _ = write_frame(
        &mut *stream,
        &D2C::Control(D2CControl::TuiFollowFailed {
          message: format!("No TUI {tui_id} found"),
        }),
      );
    }
  }

  fn handle_stop_session(&self, stream: &mut UnixStream, session_id: u64) {
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

  fn handle_stop_task(
    &self,
    stream: &mut UnixStream,
    project: &ProjectKey,
    task_id: u32,
    slug: &str,
  ) {
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
  u64::try_from(dur.as_millis()).unwrap_or(u64::MAX)
}

fn build_project_snapshot(
  cfg: &crate::config::AgencyConfig,
  project: &ProjectKey,
  prev: Option<&ProjectSnapshot>,
) -> ProjectSnapshot {
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
    tasks_info.push(TaskInfo {
      id: tref.id,
      slug: tref.slug.clone(),
      base_branch: fm.and_then(|f| f.base_branch),
    });
  }

  // Determine candidate tasks: Running from sessions or Completed via flags
  let running: std::collections::HashSet<(u32, String)> = sessions
    .iter()
    .filter(|s| s.status == "Running" || s.status == "Idle" || s.status == "Exited")
    .map(|s| (s.task.id, s.task.slug.clone()))
    .collect();

  let mut candidates: std::collections::HashSet<(u32, String)> = running.clone();
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
      .and_then(|ti| ti.base_branch.clone())
      .unwrap_or_else(|| head.clone());
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

  ProjectSnapshot {
    tasks: tasks_info,
    sessions: sessions_sorted,
    metrics,
  }
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

#[derive(Debug, Clone)]
struct TuiEntry {
  pid: u32,
  last_seen_ms: u64,
  focused_task_id: Option<u32>,
}

fn assign_tui_id(
  registry: &Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>,
  project_root: &str,
  pid: u32,
) -> u32 {
  let mut guard = registry.lock();
  let map = guard.entry(project_root.to_string()).or_default();
  // If pid already registered, reuse id
  if let Some((id, _)) = map.iter().find(|(_, e)| e.pid == pid) {
    return *id;
  }
  let mut id: u32 = 1;
  while map.contains_key(&id) {
    id += 1;
  }
  map.insert(
    id,
    TuiEntry {
      pid,
      last_seen_ms: now_ms(),
      focused_task_id: None,
    },
  );
  id
}

fn unregister_tui(
  registry: &Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>,
  project_root: &str,
  pid: u32,
) {
  let mut guard = registry.lock();
  if let Some(map) = guard.get_mut(project_root) {
    let target: Option<u32> = map.iter().find(|(_, e)| e.pid == pid).map(|(id, _)| *id);
    if let Some(id) = target {
      map.remove(&id);
    }
  }
}

fn get_tui(
  registry: &Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>,
  project_root: &str,
  tui_id: u32,
) -> Option<TuiEntry> {
  registry
    .lock()
    .get(project_root)
    .and_then(|m| m.get(&tui_id).cloned())
}

fn update_tui_focus(
  registry: &Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>,
  project_root: &str,
  tui_id: u32,
  task_id: u32,
) {
  if let Some(map) = registry.lock().get_mut(project_root)
    && let Some(entry) = map.get_mut(&tui_id)
  {
    entry.focused_task_id = Some(task_id);
    entry.last_seen_ms = now_ms();
  }
}

fn list_tuis(
  registry: &Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>,
  project_root: &str,
) -> Vec<crate::daemon_protocol::TuiListItem> {
  let mut out = Vec::new();
  if let Some(map) = registry.lock().get(project_root) {
    for (id, e) in map {
      out.push(crate::daemon_protocol::TuiListItem {
        tui_id: *id,
        pid: e.pid,
        focused_task_id: e.focused_task_id,
      });
    }
    out.sort_by(|a, b| a.tui_id.cmp(&b.tui_id));
  }
  out
}

fn prune_dead_tuis(registry: &Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>>) {
  use std::process::Command;
  let mut guard = registry.lock();
  for (_proj, map) in guard.iter_mut() {
    let ids: Vec<u32> = map
      .iter()
      .filter_map(|(id, e)| {
        let pid_str = e.pid.to_string();
        match Command::new("kill").arg("-0").arg(&pid_str).status() {
          Ok(st) if st.success() => None,
          _ => Some(*id),
        }
      })
      .collect();
    for id in ids {
      map.remove(&id);
    }
  }
}

fn broadcast_tui_focus(
  subs: &Arc<Mutex<Vec<Subscriber>>>,
  project: &ProjectKey,
  tui_id: u32,
  task_id: u32,
) {
  let mut remove_idx = Vec::new();
  let mut subs_list = subs.lock();
  for (i, sub) in subs_list.iter_mut().enumerate() {
    if sub.project.repo_root == project.repo_root
      && write_frame(
        &mut sub.stream,
        &D2C::Control(D2CControl::TuiFocusTaskChanged {
          project: project.clone(),
          tui_id,
          task_id,
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn assign_and_reuse_ids_and_list_sorting() {
    let reg: Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>> =
      Arc::new(Mutex::new(HashMap::new()));
    let proj = "/tmp/proj";
    // Assign sequential ids for distinct PIDs
    let id1 = assign_tui_id(&reg, proj, 111);
    let id2 = assign_tui_id(&reg, proj, 222);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    // Reuse id for same PID
    let id1b = assign_tui_id(&reg, proj, 111);
    assert_eq!(id1b, id1);

    // List should contain both, sorted by tui_id
    let list = list_tuis(&reg, proj);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].tui_id, 1);
    assert_eq!(list[1].tui_id, 2);

    // Unregister PID 111 and ensure only one remains
    unregister_tui(&reg, proj, 111);
    let list2 = list_tuis(&reg, proj);
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].tui_id, 2);
  }

  #[test]
  fn prune_dead_removes_inactive_pids() {
    let reg: Arc<Mutex<HashMap<String, HashMap<u32, TuiEntry>>>> =
      Arc::new(Mutex::new(HashMap::new()));
    let proj = "/tmp/proj";
    let alive = std::process::id();
    {
      let mut g = reg.lock();
      let m = g.entry(proj.to_string()).or_default();
      m.insert(
        1,
        TuiEntry {
          pid: alive,
          last_seen_ms: 0,
          focused_task_id: None,
        },
      );
      // A likely-nonexistent pid (best-effort)
      m.insert(
        2,
        TuiEntry {
          pid: 999_987_654,
          last_seen_ms: 0,
          focused_task_id: None,
        },
      );
    }
    prune_dead_tuis(&reg);
    let list = list_tuis(&reg, proj);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].tui_id, 1);
  }

  #[test]
  fn broadcast_focus_writes_frame_to_subscriber() {
    use std::os::unix::net::UnixStream as US;
    let subs: Arc<Mutex<Vec<Subscriber>>> = Arc::new(Mutex::new(Vec::new()));
    let (a, mut b) = US::pair().expect("pair");
    let pk = ProjectKey {
      repo_root: "/tmp/proj".to_string(),
    };
    subs.lock().push(Subscriber {
      project: pk.clone(),
      stream: a.try_clone().unwrap(),
    });

    // Send a focus change
    broadcast_tui_focus(&subs, &pk, 3, 42);

    // Read the frame from the other end
    let msg: D2C = read_frame(&mut b).expect("frame");
    match msg {
      D2C::Control(D2CControl::TuiFocusTaskChanged {
        project,
        tui_id,
        task_id,
      }) => {
        assert_eq!(project.repo_root, pk.repo_root);
        assert_eq!(tui_id, 3);
        assert_eq!(task_id, 42);
      }
      other @ D2C::Control(_) => panic!("unexpected: {other:?}"),
    }
  }
}
