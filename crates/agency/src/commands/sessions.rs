use anyhow::{Context, Result};
use std::os::unix::net::UnixStream;

use crate::config::{AppContext, compute_socket_path};
use crate::pty::protocol::{C2D, C2DControl, D2C, D2CControl, ProjectKey, read_frame, write_frame};
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::term::print_table;

pub fn run(ctx: &AppContext) -> Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let mut stream = UnixStream::connect(&socket)
    .with_context(|| format!("failed to connect to {}", socket.display()))?;

  // Filter by current project
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };

  write_frame(
    &mut stream,
    &C2D::Control(C2DControl::ListSessions {
      project: Some(project),
    }),
  )?;

  let reply: D2C = read_frame(&mut stream)?;
  match reply {
    D2C::Control(D2CControl::Sessions { entries }) => {
      let headers = ["SESSION", "TASK", "CLIENTS", "STATUS", "CWD"];
      let rows: Vec<Vec<String>> = entries
        .into_iter()
        .map(|e| {
          vec![
            e.session_id.to_string(),
            format!("{}-{}", e.task.id, e.task.slug),
            e.clients.to_string(),
            e.status,
            e.cwd,
          ]
        })
        .collect();
      print_table(&headers, &rows);
    }
    D2C::Control(D2CControl::Error { message }) => {
      anyhow::bail!("Daemon error: {message}");
    }
    _ => {
      anyhow::bail!("Protocol error: Expected Sessions reply");
    }
  }

  Ok(())
}
