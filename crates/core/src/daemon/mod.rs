use std::path::{Path, PathBuf};
use std::{fs, io};

use chrono::Utc;
use jsonrpsee::core::RpcResult;
use jsonrpsee::server::{self, RpcModule};
use jsonrpsee::types::ErrorObjectOwned;
use tokio::net::UnixListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::adapters::{fs as fsutil, git as gitutil};
use crate::domain::task::{Status, Task, TaskFrontMatter, TaskId};
use crate::rpc::{
  DaemonStatus, PtyAttachParams, PtyAttachResult, PtyDetachParams, PtyInputParams, PtyReadParams,
  PtyReadResult, PtyResizeParams, TaskInfo, TaskListParams, TaskListResponse, TaskNewParams,
  TaskRef, TaskStartParams, TaskStartResult,
};

/// Handle to the running daemon server.
pub struct DaemonHandle {
  task: JoinHandle<()>,
  socket_path: PathBuf,
  // Keep the server handle alive to prevent immediate shutdown
  _server_handle: server::ServerHandle,
}

impl DaemonHandle {
  /// Stop the daemon task and remove the socket file if it exists.
  pub fn stop(self) {
    self.task.abort();
    let _ = fs::remove_file(&self.socket_path);
  }

  /// Await the daemon task to finish (e.g., after shutdown).
  pub async fn wait(self) {
    let _ = self.task.await;
  }

  /// Get the socket path the daemon is bound to.
  pub fn socket_path(&self) -> &Path {
    &self.socket_path
  }
}

fn next_task_id(tasks_dir: &Path) -> io::Result<u64> {
  let mut max_id = 0u64;
  if tasks_dir.exists() {
    for entry in fs::read_dir(tasks_dir)? {
      let entry = entry?;
      let name = entry.file_name();
      let name = name.to_string_lossy();
      if let Ok((TaskId(id), _slug)) = Task::parse_filename(&name)
        && id > max_id
      {
        max_id = id;
      }
    }
  }
  Ok(max_id + 1)
}

fn read_task_info(path: &Path, id: u64, slug: String) -> io::Result<TaskInfo> {
  let s = fs::read_to_string(path)?;
  let t = Task::from_markdown(TaskId(id), slug.clone(), &s)
    .map_err(|e| io::Error::other(e.to_string()))?;
  Ok(TaskInfo {
    id,
    slug,
    status: t.front_matter.status,
  })
}

fn find_task_path_by_ref(project_root: &Path, r: &TaskRef) -> io::Result<(PathBuf, u64, String)> {
  let dir = fsutil::tasks_dir(project_root);
  let mut found: Option<(PathBuf, u64, String)> = None;
  for entry in fs::read_dir(&dir)? {
    let entry = entry?;
    let name = entry.file_name();
    let name = name.to_string_lossy().to_string();
    if let Ok((TaskId(id), slug)) = Task::parse_filename(&name) {
      let mut ok = false;
      if let Some(want) = r.id {
        ok = want == id;
      }
      if !ok && let Some(wslug) = &r.slug {
        ok = &slug == wslug;
      }
      if ok {
        found = Some((entry.path(), id, slug));
        break;
      }
    }
  }
  found.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "task not found"))
}

fn resume_running_tasks_if_configured() {
  if let Some(root_os) = std::env::var_os("AGENCY_RESUME_ROOT") {
    let root = PathBuf::from(root_os);
    if !root.exists() {
      return;
    }
    let tasks_dir = fsutil::tasks_dir(&root);
    if !tasks_dir.exists() {
      return;
    }
    info!(event = "daemon_resume_scan", root = %root.display(), "scanning for running tasks to resume");
    if let Ok(read_dir) = fs::read_dir(&tasks_dir) {
      for entry in read_dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if let Ok((TaskId(id), slug)) = Task::parse_filename(&name)
          && let Ok(s) = fs::read_to_string(entry.path())
          && let Ok(task) = Task::from_markdown(TaskId(id), slug.clone(), &s)
          && task.front_matter.status == Status::Running
        {
          let wt = fsutil::worktree_path(&root, id, &slug);
          let _ = fs::create_dir_all(&wt);
          match crate::adapters::pty::ensure_spawn(&root, id, &wt) {
            Ok(()) => info!(event = "daemon_resume_ok", id, slug = %slug, "resumed running task"),
            Err(e) => {
              warn!(event = "daemon_resume_fail", id, slug = %slug, error = %e, "failed to resume task")
            }
          }
        }
      }
    }
  }
}

/// Start a JSON-RPC server over a Unix domain socket using jsonrpsee.
/// Supports methods `daemon.status` and `daemon.shutdown` and task.* (Phase 9).
pub async fn start(socket_path: &Path) -> io::Result<DaemonHandle> {
  if let Some(parent) = socket_path.parent() {
    fs::create_dir_all(parent)?;
  }
  // Remove stale socket if present
  let _ = fs::remove_file(socket_path);

  let listener = UnixListener::bind(socket_path)?;
  let sock = socket_path.to_path_buf();

  // Shutdown signal channel
  let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

  // Build jsonrpsee module with context of the socket path
  let mut module = RpcModule::new(sock.clone());
  module
    .register_method("daemon.status", |_params, ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let status = DaemonStatus { version: env!("CARGO_PKG_VERSION").to_string(), pid: std::process::id(), socket_path: ctx.display().to_string() };
      info!(event = "daemon_status", pid = status.pid, socket = %status.socket_path, version = %status.version, "status served");
      Ok(serde_json::to_value(status).unwrap())
    })
    .expect("register daemon.status");

  let (stop_handle, _server_handle) = server::stop_channel();
  let _stop_handle_for_shutdown = stop_handle.clone();
  let shutdown_tx_for_shutdown = shutdown_tx.clone();

  module
    .register_method(
      "daemon.shutdown",
      move |_params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        info!(
          event = "daemon_shutdown_requested",
          "shutdown requested via RPC"
        );
        // Signal accept loop to exit; existing connections will drain
        let _ = shutdown_tx_for_shutdown.send(true);
        Ok(serde_json::json!(true))
      },
    )
    .expect("register daemon.shutdown");

  // ---- task.new ----
  module
    .register_method(
      "task.new",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: TaskNewParams = params.parse()?;
        let root = PathBuf::from(&p.project_root);
        fsutil::ensure_layout(&root)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let tasks_dir = fsutil::tasks_dir(&root);
        let id = next_task_id(&tasks_dir)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let slug = p.slug;
        let fm = TaskFrontMatter {
          base_branch: p.base_branch,
          status: Status::Draft,
          labels: p.labels,
          created_at: Utc::now(),
          agent: p.agent,
          session_id: None,
        };
        let task = Task {
          id: TaskId(id),
          slug: slug.clone(),
          front_matter: fm,
          body: p.body.unwrap_or_default(),
        };
        let md = task
          .to_markdown()
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        let file_path = tasks_dir.join(Task::format_filename(task.id, &slug));
        fs::write(&file_path, md)
          .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
        info!(event = "task_new", id, slug = %slug, path = %file_path.display(), "task created");
        let info = TaskInfo {
          id,
          slug,
          status: Status::Draft,
        };
        Ok(serde_json::to_value(info).unwrap())
      },
    )
    .expect("register task.new");

  // ---- task.status ----
  module
    .register_method(
      "task.status",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: TaskListParams = params.parse()?;
        let root = PathBuf::from(&p.project_root);
        let tasks_dir = fsutil::tasks_dir(&root);
        let mut tasks: Vec<TaskInfo> = Vec::new();
        if tasks_dir.exists() {
          for entry in fs::read_dir(&tasks_dir)
            .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?
          {
            let entry =
              entry.map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Ok((TaskId(id), slug)) = Task::parse_filename(&name)
              && let Ok(info) = read_task_info(&entry.path(), id, slug)
            {
              tasks.push(info);
            }
          }
        }
        tasks.sort_by_key(|t| t.id);
        let resp = TaskListResponse { tasks };
        Ok(serde_json::to_value(resp).unwrap())
      },
    )
    .expect("register task.status");

  // ---- task.start (ensure git worktree + PTY spawn) ----
  module
    .register_method("task.start", |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let p: TaskStartParams = params.parse()?;
      let root = PathBuf::from(&p.project_root);
      let (path, id, slug) = find_task_path_by_ref(&root, &p.task).map_err(|e| ErrorObjectOwned::owned(-32001, e.to_string(), None::<()>))?;
      // Load task and open repo with clear error if not a git repo
      let s = fs::read_to_string(&path).map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      let mut task = Task::from_markdown(TaskId(id), slug.clone(), &s).map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      let repo = match git2::Repository::open(&root) {
        Ok(r) => r,
        Err(_) => {
          return Err(ErrorObjectOwned::owned(-32002, "not a git repository", None::<()>));
        }
      };
      // Validate base branch tip and ensure worktree exists on the task branch
      let base_sha = gitutil::resolve_base_branch_tip(&repo, &task.front_matter.base_branch).map_err(|e| ErrorObjectOwned::owned(-32003, e.to_string(), None::<()>))?;
      info!(event = "task_start_validated", id, slug = %slug, base_branch = %task.front_matter.base_branch, base_sha = %base_sha.to_string(), "validated git base");
      // Ensure real git worktree and set PTY cwd to it
      let wt = gitutil::ensure_task_worktree(&repo, &root, id, &slug, &task.front_matter.base_branch)
        .map_err(|e| ErrorObjectOwned::owned(-32005, e.to_string(), None::<()>))?;
      // Transition to running and persist
      task.transition_to(Status::Running).map_err(|e| ErrorObjectOwned::owned(-32004, e.to_string(), None::<()>))?;
      let md = task.to_markdown().map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      fs::write(&path, md).map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      // Spawn PTY session for this task in the worktree
      { let _ = crate::adapters::pty::ensure_spawn(&root, id, &wt); }
      let res = TaskStartResult { id, slug, status: Status::Running };
      Ok(serde_json::to_value(res).unwrap())
    })
    .expect("register task.start");

  // ---- pty.attach ----
  module
    .register_method("pty.attach", |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let p: PtyAttachParams = params.parse()?;
      let root = PathBuf::from(&p.project_root);
      let (path, id, _slug) = find_task_path_by_ref(&root, &p.task).map_err(|e| ErrorObjectOwned::owned(-32001, e.to_string(), None::<()>))?;
      // Enforce running state; do not spawn here
      let s = fs::read_to_string(&path).map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      let task = Task::from_markdown(TaskId(id), "_".into(), &s).map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      if task.front_matter.status != Status::Running {
        return Err(ErrorObjectOwned::owned(-32010, format!("cannot attach: task is not running (status: {:?})", task.front_matter.status), None::<()>));
      }
      // Attach to existing session
      let attach_id = crate::adapters::pty::attach(&root, id).map_err(|e| ErrorObjectOwned::owned(-32010, e.to_string(), None::<()>))?;
      // Apply initial size
      let _ = crate::adapters::pty::resize(&attach_id, p.rows, p.cols);
      tracing::info!(event = "pty_attach", task_id = id, attachment_id = %attach_id, rows = p.rows, cols = p.cols, "pty attached");
      let res = PtyAttachResult { attachment_id: attach_id };
      Ok(serde_json::to_value(res).unwrap())
    })
    .expect("register pty.attach");

  // ---- pty.read ----
  module
    .register_method(
      "pty.read",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyReadParams = params.parse()?;
        let (data, eof) = crate::adapters::pty::read(&p.attachment_id, p.max_bytes, p.wait_ms)
          .map_err(|e| ErrorObjectOwned::owned(-32011, e.to_string(), None::<()>))?;
        let text = String::from_utf8_lossy(&data).to_string();
        let res = PtyReadResult { data: text, eof };
        Ok(serde_json::to_value(res).unwrap())
      },
    )
    .expect("register pty.read");

  // ---- pty.tick ----
  module
    .register_method(
      "pty.tick",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: crate::rpc::PtyTickParams = params.parse()?;
        if let Some(ref input) = p.input {
          debug!(event = "daemon_pty_tick_input", attachment_id = %p.attachment_id, bytes = input.len());
          crate::adapters::pty::input(&p.attachment_id, input.as_bytes()).map_err(|e| ErrorObjectOwned::owned(-32012, e.to_string(), None::<()>))?;
        }
        if let Some((rows, cols)) = p.resize {
          debug!(event = "daemon_pty_tick_resize", attachment_id = %p.attachment_id, rows, cols);
          crate::adapters::pty::resize(&p.attachment_id, rows, cols).map_err(|e| ErrorObjectOwned::owned(-32013, e.to_string(), None::<()>))?;
        }
        let (data, eof) = crate::adapters::pty::read(&p.attachment_id, p.max_bytes, p.wait_ms).map_err(|e| ErrorObjectOwned::owned(-32011, e.to_string(), None::<()>))?;
        debug!(event = "daemon_pty_tick_read", attachment_id = %p.attachment_id, bytes = data.len(), eof, wait_ms = p.wait_ms, max_bytes = ?p.max_bytes);
        let text = String::from_utf8_lossy(&data).to_string();
        let res = PtyReadResult { data: text, eof };
        Ok(serde_json::to_value(res).unwrap())
      },
    )
    .expect("register pty.tick");

  // ---- pty.input ----
  module
    .register_method(
      "pty.input",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyInputParams = params.parse()?;
        debug!(event = "daemon_pty_input", attachment_id = %p.attachment_id, bytes = p.data.len());
        crate::adapters::pty::input(&p.attachment_id, p.data.as_bytes())
          .map_err(|e| ErrorObjectOwned::owned(-32012, e.to_string(), None::<()>))?;
        Ok(serde_json::json!(true))
      },
    )
    .expect("register pty.input");

  // ---- pty.resize ----
  module
    .register_method("pty.resize", |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let p: PtyResizeParams = params.parse()?;
      crate::adapters::pty::resize(&p.attachment_id, p.rows, p.cols).map_err(|e| ErrorObjectOwned::owned(-32013, e.to_string(), None::<()>))?;
      tracing::info!(event = "pty_resize", attachment_id = %p.attachment_id, rows = p.rows, cols = p.cols, "pty resized");
      Ok(serde_json::json!(true))
    })
    .expect("register pty.resize");

  // ---- pty.detach ----
  module
    .register_method(
      "pty.detach",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyDetachParams = params.parse()?;
        crate::adapters::pty::detach(&p.attachment_id)
          .map_err(|e| ErrorObjectOwned::owned(-32014, e.to_string(), None::<()>))?;
        tracing::info!(event = "pty_detach", attachment_id = %p.attachment_id, "pty detached");
        Ok(serde_json::json!(true))
      },
    )
    .expect("register pty.detach");

  // Before serving, resume running tasks for configured resume root
  resume_running_tasks_if_configured();

  let svc_builder = server::Server::builder().to_service_builder();

  info!(event = "daemon_started", socket = %socket_path.display(), "daemon server started");

  let task = tokio::spawn(async move {
    loop {
      tokio::select! {
        _ = shutdown_rx.changed() => {
          info!(event = "daemon_shutdown", "shutdown signal received; stopping accept loop");
          break;
        }
        res = listener.accept() => {
          match res {
            Ok((stream, _addr)) => {
              let methods = module.clone();
              let svc = svc_builder.clone().build(methods, stop_handle.clone());
              // Serve the UnixStream (HTTP over UDS)
              tokio::spawn(async move {
                if let Err(e) = server::serve(stream, svc).await {
                  error!(error = %e, "serve error");
                }
              });
            }
            Err(e) => {
              error!(error = %e, "accept error");
              break;
            }
          }
        }
      }
    }
    // Best-effort cleanup
    let _ = fs::remove_file(&sock);
    info!(event = "daemon_stopped", socket = %sock.display(), "daemon server stopped");
  });

  Ok(DaemonHandle {
    task,
    socket_path: socket_path.to_path_buf(),
    _server_handle,
  })
}
