use std::fs;

use anyhow::{Context, Result};
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::config::compute_socket_path;
use crate::pty::protocol::{C2D, C2DControl, ProjectKey, write_frame};
use crate::utils::git::{delete_branch_if_exists, open_main_repo, prune_worktree_if_exists};
use crate::utils::task::{branch_name, resolve_id_or_slug, task_file, worktree_dir, worktree_name};
use crate::utils::term::confirm;

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let branch = branch_name(&tref);
  let wt_dir = worktree_dir(&ctx.paths, &tref);
  let file = task_file(&ctx.paths, &tref);

  anstream::println!(
    "{}\n  file: {}\n  branch: {}\n  worktree: {}",
    "About to remove:".yellow(),
    file.display().to_string().cyan(),
    branch.cyan(),
    wt_dir.display().to_string().cyan(),
  );

  if confirm("Proceed? [y/N]")? {
    let repo = open_main_repo(ctx.paths.cwd())?;
    let _ = prune_worktree_if_exists(&repo, &wt_dir)?;
    let _ = delete_branch_if_exists(&repo, &branch)?;
    if file.exists() {
      fs::remove_file(&file).with_context(|| format!("failed to remove {}", file.display()))?;
    }
    anstream::println!("{}", "Removed task, branch, and worktree".green());

    // Best-effort notify daemon to stop sessions for this task
    let socket = compute_socket_path(&ctx.config);
    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&socket) {
      let repo_root = repo
        .workdir()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
        .unwrap_or(ctx.paths.cwd().clone());
      let project = ProjectKey {
        repo_root: repo_root.display().to_string(),
      };
      let _ = write_frame(
        &mut stream,
        &C2D::Control(C2DControl::StopTask {
          project,
          task_id: tref.id,
          slug: tref.slug.clone(),
        }),
      );
      let _ = stream.shutdown(std::net::Shutdown::Both);
    }
  } else {
    anstream::println!("{}", "Cancelled".yellow());
  }

  Ok(())
}
