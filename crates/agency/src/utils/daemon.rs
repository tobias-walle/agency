use crate::config::{AppContext, compute_socket_path};
use crate::pty::protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionInfo, read_frame, write_frame,
};
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::TaskRef;
use anyhow::{Context, Result};
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Sends exactly one control message to the daemon over a short-lived connection.
///
/// Protocol: the daemon reads one control frame, replies, then closes the socket.
/// Control commands are one-shot (`StopTask`, `StopSession`, `ListSessions`, Shutdown);
/// opening a new connection per message avoids ambiguity with attach (multi-frame)
/// flows and prevents protocol state mismatches.
pub fn send_message_to_daemon(socket: &Path, msg: C2DControl) -> Result<()> {
  let mut stream = UnixStream::connect(socket)
    .with_context(|| format!("failed to connect to {}", socket.display()))?;
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

/// Best-effort helper to notify the daemon that tasks changed for this project.
pub fn notify_tasks_changed(ctx: &AppContext) -> anyhow::Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  send_message_to_daemon(&socket, C2DControl::NotifyTasksChanged { project })
}

/// Best-effort helper to list sessions for the current project.
///
/// Returns an empty list if the daemon is unavailable or any error occurs.
#[must_use]
pub fn list_sessions_for_project(ctx: &AppContext) -> Vec<SessionInfo> {
  let socket = compute_socket_path(&ctx.config);
  // Compute project key from the main repo workdir
  let Ok(repo) = open_main_repo(ctx.paths.cwd()) else {
    return Vec::new();
  };
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  // Connect and request sessions; swallow errors
  let Ok(mut stream) = UnixStream::connect(&socket) else {
    return Vec::new();
  };
  if write_frame(
    &mut stream,
    &C2D::Control(C2DControl::ListSessions {
      project: Some(project),
    }),
  )
  .is_err()
  {
    return Vec::new();
  }

  let reply: Result<D2C> = read_frame(&mut stream);
  match reply {
    Ok(D2C::Control(D2CControl::Sessions { entries })) => entries,
    _ => Vec::new(),
  }
}
