use anyhow::Result;

use crate::config::{AppContext, compute_socket_path};
use crate::pty::protocol::{C2D, C2DControl, D2C, D2CControl, ProjectKey, read_frame, write_frame};
use crate::utils::daemon::connect_daemon_socket;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::resolve_id_or_slug;
use crate::{log_info, log_success};
// Use macros via module path

pub fn run(ctx: &AppContext, ident: Option<&str>, session_id: Option<u64>) -> Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let mut stream = connect_daemon_socket(&socket)?;

  if let Some(sid) = session_id {
    write_frame(
      &mut stream,
      &C2D::Control(C2DControl::StopSession { session_id: sid }),
    )?;
    // Read Goodbye acknowledgement, then log
    match read_frame::<_, D2C>(&mut stream) {
      Ok(D2C::Control(D2CControl::Goodbye)) => {
        log_success!("Stopped session {}", sid);
      }
      Ok(D2C::Control(D2CControl::Error { message })) => {
        anyhow::bail!("Daemon error: {message}");
      }
      _ => {
        // Silent success if protocol differs; keep user informed
        log_info!("Requested stop for session {}", sid);
      }
    }
    return Ok(());
  }

  if let Some(task_ident) = ident {
    let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
    let repo = open_main_repo(ctx.paths.cwd())?;
    let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
    let project = ProjectKey {
      repo_root: repo_root.display().to_string(),
    };
    write_frame(
      &mut stream,
      &C2D::Control(C2DControl::StopTask {
        project,
        task_id: task.id,
        slug: task.slug.clone(),
      }),
    )?;
    // Read ack and log count
    match read_frame::<_, D2C>(&mut stream) {
      Ok(D2C::Control(D2CControl::Ack { stopped })) => {
        log_success!(
          "Stopped {} session(s) for {}-{}",
          stopped,
          task.id,
          task.slug
        );
      }
      Ok(D2C::Control(D2CControl::Error { message })) => {
        anyhow::bail!("Daemon error: {message}");
      }
      _ => {
        log_info!("Requested stop for task {}-{}", task.id, task.slug);
      }
    }
    return Ok(());
  }

  anyhow::bail!("Must specify --session <id> or task ident")
}
