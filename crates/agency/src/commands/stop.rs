use anyhow::{Context, Result};
use std::os::unix::net::UnixStream;

use crate::config::{AppContext, compute_socket_path};
use crate::pty::protocol::{C2D, C2DControl, D2C, D2CControl, ProjectKey, read_frame, write_frame};
use crate::utils::git::open_main_repo;
use crate::utils::task::resolve_id_or_slug;

pub fn run(ctx: &AppContext, ident: Option<&str>, session_id: Option<u64>) -> Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let mut stream = UnixStream::connect(&socket)
    .with_context(|| format!("failed to connect to {}", socket.display()))?;

  if let Some(sid) = session_id {
    write_frame(
      &mut stream,
      &C2D::Control(C2DControl::StopSession { session_id: sid }),
    )?;
    // Best-effort read Goodbye
    if let Ok(D2C::Control(D2CControl::Goodbye)) = read_frame(&mut stream) {}
    return Ok(());
  }

  if let Some(task_ident) = ident {
    let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
    let repo = open_main_repo(ctx.paths.cwd())?;
    let repo_root = repo
      .workdir()
      .map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
      .unwrap_or(ctx.paths.cwd().clone());
    let project = ProjectKey {
      repo_root: repo_root.display().to_string(),
    };
    write_frame(
      &mut stream,
      &C2D::Control(C2DControl::StopTask {
        project,
        task_id: task.id,
        slug: task.slug,
      }),
    )?;
    return Ok(());
  }

  anyhow::bail!("must specify --session <id> or task ident")
}
