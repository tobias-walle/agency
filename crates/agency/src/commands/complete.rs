use anyhow::Result;

use crate::config::AppContext;
use crate::log_success;
use crate::utils::log::t;
use crate::utils::status::mark_task_completed;
use crate::utils::task::resolve_id_or_slug;

/// Mark a task as Completed.
///
/// When `ident` is `None`, attempts to read `AGENCY_TASK_ID` from the environment
/// to resolve the current task context. Bails with guidance when unavailable.
pub fn run(ctx: &AppContext, ident: Option<&str>) -> Result<()> {
  let tref = match ident {
    Some(i) => resolve_id_or_slug(&ctx.paths, i)?,
    None => {
      let id_env = std::env::var("AGENCY_TASK_ID").map_err(|_| {
        anyhow::anyhow!("Not running in an agency environment. Cannot complete task")
      })?;
      resolve_id_or_slug(&ctx.paths, &id_env)?
    }
  };

  mark_task_completed(&ctx.paths, &tref)?;
  log_success!(
    "Marked task {} {} as Completed",
    t::id(tref.id),
    t::slug(&tref.slug)
  );
  Ok(())
}
