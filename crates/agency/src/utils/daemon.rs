use crate::config::{AppContext, compute_socket_path};
use crate::daemon_protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionInfo, read_frame, write_frame,
};
use crate::log_warn;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::TaskRef;
use anyhow::{Context, Result, anyhow, bail};
use std::os::unix::net::UnixStream;
use std::path::Path;

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
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
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
fn notify_tasks_changed(ctx: &AppContext) -> anyhow::Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
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
pub fn list_sessions_for_project(ctx: &AppContext) -> anyhow::Result<Vec<SessionInfo>> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  let mut stream = connect_daemon_socket(&socket)?;
  write_frame(
    &mut stream,
    &C2D::Control(C2DControl::ListSessions {
      project: Some(project),
    }),
  )
  .context("failed to write ListSessions frame")?;

  let reply: Result<D2C> = read_frame(&mut stream);
  match reply {
    Ok(D2C::Control(D2CControl::Sessions { entries })) => Ok(entries),
    Ok(D2C::Control(D2CControl::Error { message })) => bail!("{message}"),
    Ok(_) => bail!("Protocol error: Expected Sessions reply"),
    Err(err) => Err(err),
  }
}
