use std::fs;

use anyhow::{Context, Result};
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::utils::daemon::stop_sessions_of_task;
use crate::utils::git::{delete_branch_if_exists, open_main_repo, prune_worktree_if_exists};
use crate::utils::task::{branch_name, resolve_id_or_slug, task_file, worktree_dir};
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
    let _ = stop_sessions_of_task(ctx, &tref);
  } else {
    anstream::println!("{}", "Cancelled".yellow());
  }

  Ok(())
}
