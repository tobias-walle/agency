use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::session::{build_session_plan, start_session_for_task};
use crate::utils::task::resolve_id_or_slug;
use anyhow::Result;

/// Start a task's session; optionally attach. Fails if already started.
///
/// Performs the same preparation as `attach` (ensure branch/worktree, compute agent cmd),
/// then optionally attaches to the daemon sending `OpenSession` with the real terminal size.
pub fn run_with_attach(ctx: &AppContext, ident: &str, attach: bool) -> Result<()> {
  // Resolve task
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  // Fail when a session is already running for this task
  let existing = get_project_state(ctx)?
    .sessions
    .into_iter()
    .any(|e| e.task.id == task.id && e.task.slug == task.slug);
  if existing {
    anyhow::bail!("Already started. Use attach");
  }
  let plan = build_session_plan(ctx, &task)?;
  crate::utils::daemon::notify_after_task_change(ctx, || start_session_for_task(ctx, &plan, attach))
}
