use std::fs;

use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::utils::daemon::{notify_after_task_change, stop_sessions_of_task};
use crate::utils::git::{delete_branch_if_exists, open_main_repo, prune_worktree_if_exists};
use crate::utils::task::{branch_name, resolve_id_or_slug, task_file, worktree_dir};
use crate::utils::term::confirm;
use crate::{log_success, log_warn};
// Use macros via module path

pub fn run(ctx: &AppContext, ident: &str, yes: bool) -> Result<()> {
  if yes {
    let tref = resolve_id_or_slug(&ctx.paths, ident)?;
    log_warn!("Remove task {}-{}", tref.id, tref.slug);
    run_force(ctx, ident)
  } else {
    let tref = resolve_id_or_slug(&ctx.paths, ident)?;
    let branch = branch_name(&tref);
    let wt_dir = worktree_dir(&ctx.paths, &tref);
    let file = task_file(&ctx.paths, &tref);

    log_warn!("Remove task {}-{}", tref.id, tref.slug);

    if confirm("Proceed? [y/N]")? {
      notify_after_task_change(ctx, || {
        let repo = open_main_repo(ctx.paths.cwd())?;
        let _ = prune_worktree_if_exists(&repo, &wt_dir)?;
        let _ = delete_branch_if_exists(&repo, &branch)?;
        if file.exists() {
          fs::remove_file(&file).with_context(|| format!("failed to remove {}", file.display()))?;
        }
        log_success!("Removed task, branch, and worktree");

        let _ = stop_sessions_of_task(ctx, &tref);
        Ok(())
      })?;
    } else {
      log_warn!("Cancelled");
    }

    Ok(())
  }
}

/// Remove without interactive confirmation. Intended for non-interactive TUI use.
pub fn run_force(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let branch = branch_name(&tref);
  let wt_dir = worktree_dir(&ctx.paths, &tref);
  let file = task_file(&ctx.paths, &tref);
  notify_after_task_change(ctx, || {
    let repo = open_main_repo(ctx.paths.cwd())?;
    let _ = prune_worktree_if_exists(&repo, &wt_dir)?;
    let _ = delete_branch_if_exists(&repo, &branch)?;
    if file.exists() {
      fs::remove_file(&file).with_context(|| format!("failed to remove {}", file.display()))?;
    }
    // Condensed confirmation for TUI log
    log_success!("Removed task {}-{}", tref.id, tref.slug);

    let _ = stop_sessions_of_task(ctx, &tref);
    Ok(())
  })
}
