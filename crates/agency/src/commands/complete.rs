use anyhow::Result;

use crate::commands::merge::perform_merge;
use crate::config::AppContext;
use crate::utils::daemon::notify_after_task_change;
use crate::utils::git::git_workdir;
use crate::utils::log::t;
use crate::utils::task::{cleanup_task_artifacts, resolve_task_ident};
use crate::{log_info, log_success, log_warn};

/// Complete a task by merging it into base and cleaning up.
///
/// # Errors
/// Returns an error if the task cannot be resolved or cleanup fails.
/// Merge errors are ignored if the task is already up-to-date with base.
pub fn run(ctx: &AppContext, task_ident: Option<&str>, base: Option<&str>, yes: bool) -> Result<()> {
  let task = resolve_task_ident(&ctx.paths, task_ident)?;
  let ident_str = task.id.to_string();

  notify_after_task_change(ctx, || {
    // Try to merge; if already up-to-date, skip and just clean up
    let merge_result = perform_merge(ctx, &ident_str, base);
    let already_merged = match &merge_result {
      Ok(_) => false,
      Err(e) if e.to_string().contains("No changes to merge") => {
        log_info!("Task already merged with base, skipping merge");
        true
      }
      Err(_) => {
        merge_result?;
        unreachable!()
      }
    };

    log_warn!("This will delete the task branch, worktree, and file.");
    if !ctx.tty.confirm("Proceed?", true, yes)? {
      log_warn!("Cancelled");
      return Ok(());
    }

    let repo_workdir = if already_merged {
      git_workdir(ctx.paths.root())?
    } else {
      merge_result?.repo_workdir
    };

    cleanup_task_artifacts(ctx, &task, &repo_workdir)?;
    log_success!(
      "Task {} {} {}",
      t::id(task.id),
      t::slug(&task.slug),
      if already_merged { "cleaned up" } else { "merged and cleaned up" }
    );
    Ok(())
  })
}

/// Complete a task without interactive confirmation. Intended for TUI use.
///
/// # Errors
/// Returns an error if the task cannot be resolved or cleanup fails.
pub fn run_force(ctx: &AppContext, task_ident: &str, base: Option<&str>) -> Result<()> {
  run(ctx, Some(task_ident), base, true)
}
