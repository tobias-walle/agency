use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::notify_after_task_change;
use crate::utils::git::git_workdir;
use crate::utils::log::t;
use crate::utils::task::{cleanup_task_artifacts, resolve_id_or_slug};
use crate::{log_success, log_warn};

pub fn run(ctx: &AppContext, task_ident: &str, yes: bool) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;

  log_warn!("Remove task {} {}", t::id(task.id), t::slug(&task.slug));

  if !ctx.tty.confirm("Proceed?", true, yes)? {
    log_warn!("Cancelled");
    return Ok(());
  }

  notify_after_task_change(ctx, || {
    let repo_workdir = git_workdir(ctx.paths.root())?;
    cleanup_task_artifacts(ctx, &task, &repo_workdir)?;
    log_success!(
      "Task {} {} removed",
      t::id(task.id),
      t::slug(&task.slug)
    );
    Ok(())
  })
}

/// Remove without interactive confirmation. Intended for non-interactive TUI use.
pub fn run_force(ctx: &AppContext, task_ident: &str) -> Result<()> {
  run(ctx, task_ident, true)
}
