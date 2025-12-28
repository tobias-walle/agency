use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::notify_tasks_changed;
use crate::utils::files::{remove_file, resolve_file};
use crate::utils::log::t;
use crate::utils::task::resolve_id_or_slug;
use crate::{log_success, log_warn};

pub fn run(ctx: &AppContext, task_ident: &str, file_ident: &str, yes: bool) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
  let file = resolve_file(&ctx.paths, &task, file_ident)?;

  log_warn!(
    "Remove file {} {} from task {}",
    t::id(file.id),
    t::path(&file.name),
    t::slug(&task.slug)
  );

  if !ctx.tty.confirm("Proceed?", true, yes)? {
    log_warn!("Cancelled");
    return Ok(());
  }

  remove_file(&ctx.paths, &task, &file)?;
  log_success!(
    "File {} {} removed",
    t::id(file.id),
    t::path(&file.name)
  );
  let _ = notify_tasks_changed(ctx);

  Ok(())
}
