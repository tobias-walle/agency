use anyhow::Result;

use crate::config::AppContext;
use crate::log_info;
use crate::log_success;
use crate::utils::daemon::{notify_after_task_change, stop_sessions_of_task};
use crate::utils::git::{delete_branch_if_exists, open_main_repo, prune_worktree_if_exists};
use crate::utils::log::t;
use crate::utils::task::{branch_name, resolve_id_or_slug, worktree_dir};

/// Reset a task's workspace by pruning its worktree and deleting its branch.
/// Keeps the markdown file intact. Best-effort stop of running sessions first.
pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;

  // Best-effort stop of running sessions for this task
  let _ = stop_sessions_of_task(ctx, &tref);
  log_info!(
    "Requested stop for task {}-{}",
    t::id(tref.id),
    t::slug(&tref.slug)
  );

  notify_after_task_change(ctx, || {
    let repo = open_main_repo(ctx.paths.cwd())?;
    let branch = branch_name(&tref);
    let wt_dir = worktree_dir(&ctx.paths, &tref);

    if prune_worktree_if_exists(&repo, &wt_dir).is_ok() {
      log_success!("Pruned worktree {}", t::path(wt_dir.display()));
    }
    if delete_branch_if_exists(&repo, &branch).is_ok() {
      log_success!("Deleted branch {}", branch);
    }

    Ok(())
  })
}
