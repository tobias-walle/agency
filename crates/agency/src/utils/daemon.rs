use crate::config::{AppContext, compute_socket_path};
use crate::daemon_protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionInfo, TaskInfo, TaskMetrics, TuiListItem,
  read_frame, write_frame,
};
use crate::log_warn;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::TaskRef;
use anyhow::{Context, Result, anyhow, bail};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(test)]
static TASK_NOTIFY_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
#[allow(dead_code)]
pub fn reset_task_notify_metrics() {
  TASK_NOTIFY_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(test)]
#[allow(dead_code)]
pub fn task_notify_count() -> u64 {
  TASK_NOTIFY_COUNT.load(Ordering::SeqCst)
}

const DAEMON_NOT_RUNNING_MSG: &str =
  "Daemon not running. Please start it with `agency daemon start`";

/// Connect to the daemon socket for the current context and bail with guidance on failure.
pub fn connect_daemon(ctx: &AppContext) -> anyhow::Result<UnixStream> {
  let socket = compute_socket_path(&ctx.config);
  connect_daemon_socket(&socket)
}

/// Connect to a daemon socket path and bail with guidance on failure.
pub fn connect_daemon_socket(socket: &Path) -> anyhow::Result<UnixStream> {
  UnixStream::connect(socket).map_err(|_| anyhow!(DAEMON_NOT_RUNNING_MSG))
}

/// Sends exactly one control message to the daemon over a short-lived connection.
///
/// Protocol: the daemon reads one control frame, replies, then closes the socket.
/// Control commands are one-shot (`StopTask`, `StopSession`, `ListSessions`, Shutdown);
/// opening a new connection per message avoids ambiguity with attach (multi-frame)
/// flows and prevents protocol state mismatches.
pub fn send_message_to_daemon(socket: &Path, msg: C2DControl) -> Result<()> {
  let mut stream = connect_daemon_socket(socket)?;
  write_frame(&mut stream, &C2D::Control(msg)).context("failed to write control frame")?;
  let _ = stream.shutdown(std::net::Shutdown::Both);
  Ok(())
}

/// Best-effort helper to request the daemon to stop all sessions for a task.
///
/// This does not fail the caller if the daemon is unavailable or any step
/// errors; it aims to avoid duplication of the `StopTask` notify logic.
pub fn stop_sessions_of_task(ctx: &AppContext, task: &TaskRef) -> anyhow::Result<()> {
  let socket = compute_socket_path(&ctx.config);
  // Compute project key from the main repo workdir
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  // Send a single StopTask control over a short-lived connection
  send_message_to_daemon(
    &socket,
    C2DControl::StopTask {
      project,
      task_id: task.id,
      slug: task.slug.clone(),
    },
  )
}

/// Runs a task mutation and emits a single notification when it succeeds.
pub fn notify_after_task_change<F, T>(ctx: &AppContext, operation: F) -> anyhow::Result<T>
where
  F: FnOnce() -> anyhow::Result<T>,
{
  match operation() {
    Ok(value) => {
      if let Err(err) = notify_tasks_changed(ctx) {
        log_warn!("Notify tasks changed failed: {}", err);
      }
      Ok(value)
    }
    Err(err) => Err(err),
  }
}

/// Best-effort helper to notify the daemon that tasks changed for this project.
/// This triggers a broadcast to all TUI subscribers so they can refresh.
pub fn notify_tasks_changed(ctx: &AppContext) -> anyhow::Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  let result = send_message_to_daemon(&socket, C2DControl::NotifyTasksChanged { project });
  if result.is_ok() {
    #[cfg(test)]
    {
      TASK_NOTIFY_COUNT.fetch_add(1, Ordering::SeqCst);
    }
  }
  result
}

/// Best-effort helper to list sessions for the current project.
///
/// Returns an error with guidance if the daemon is unavailable.
#[derive(Debug, Clone)]
pub struct ProjectState {
  #[allow(dead_code)]
  pub tasks: Vec<TaskInfo>,
  pub sessions: Vec<SessionInfo>,
  pub metrics: Vec<TaskMetrics>,
}

/// Best-effort helper to fetch a one-shot project state snapshot.
pub fn get_project_state(ctx: &AppContext) -> anyhow::Result<ProjectState> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  let mut stream = connect_daemon_socket(&socket)?;
  write_frame(
    &mut stream,
    &C2D::Control(C2DControl::ListProjectState { project }),
  )
  .context("failed to write ListProjectState frame")?;

  let reply: Result<D2C> = read_frame(&mut stream);
  match reply {
    Ok(D2C::Control(D2CControl::ProjectState {
      project: _,
      tasks,
      sessions,
      metrics,
    })) => Ok(ProjectState {
      tasks,
      sessions,
      metrics,
    }),
    Ok(D2C::Control(D2CControl::Error { message })) => bail!("{message}"),
    Ok(_) => bail!("Protocol error: Expected ProjectState reply"),
    Err(err) => Err(err),
  }
}

/// Register a running TUI instance and obtain a numeric id.
pub fn tui_register(ctx: &AppContext, pid: u32) -> anyhow::Result<u32> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  let mut stream = connect_daemon_socket(&socket)?;
  write_frame(
    &mut stream,
    &C2D::Control(C2DControl::TuiRegister { project, pid }),
  )?;
  match read_frame::<_, D2C>(&mut stream)? {
    D2C::Control(D2CControl::TuiRegistered { tui_id }) => Ok(tui_id),
    D2C::Control(D2CControl::Error { message }) => anyhow::bail!(message),
    D2C::Control(_) => anyhow::bail!("Protocol error: expected TuiRegistered reply"),
  }
}

pub fn tui_unregister(ctx: &AppContext, pid: u32) -> anyhow::Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  send_message_to_daemon(&socket, C2DControl::TuiUnregister { project, pid })
}

pub fn tui_list(ctx: &AppContext) -> anyhow::Result<Vec<TuiListItem>> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  let mut stream = connect_daemon_socket(&socket)?;
  write_frame(&mut stream, &C2D::Control(C2DControl::TuiList { project }))?;
  match read_frame::<_, D2C>(&mut stream)? {
    D2C::Control(D2CControl::TuiList { items }) => Ok(items),
    D2C::Control(D2CControl::Error { message }) => anyhow::bail!(message),
    D2C::Control(_) => anyhow::bail!("Protocol error: expected TuiList reply"),
  }
}

/// Ensure the daemon is running and matches the current CLI version.
///
/// - Skips when `AGENCY_NO_AUTOSTART=1` is set.
/// - Starts the daemon if the socket connect fails.
/// - If connect succeeds, queries version and restarts on mismatch or unexpected reply.
pub fn ensure_running_and_latest_version(ctx: &AppContext) -> anyhow::Result<()> {
  if std::env::var("AGENCY_NO_AUTOSTART").ok().as_deref() == Some("1") {
    return Ok(());
  }

  let socket = compute_socket_path(&ctx.config);
  match UnixStream::connect(&socket) {
    Err(_) => {
      // Not running -> start
      crate::commands::daemon::start()?;
      return Ok(());
    }
    Ok(mut stream) => {
      // Connected: query version with a short timeout
      let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
      write_frame(&mut stream, &C2D::Control(C2DControl::GetVersion))
        .context("failed to write GetVersion frame")?;
      let reply: Result<D2C> = read_frame(&mut stream);
      let cli_ver = crate::utils::version::get_version();
      let matches = match reply {
        Ok(D2C::Control(D2CControl::Version { version })) => version == cli_ver,
        _ => false,
      };
      if !matches {
        // Older daemon without version support or mismatched -> restart daemon only
        crate::commands::daemon::restart_daemon_only()?;
      }
    }
  }
  Ok(())
}
