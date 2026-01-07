use crate::config::AppContext;
use crate::utils::daemon::{get_project_state, send_start_bootstrap};
use crate::utils::session::{build_session_plan, start_session_for_task};
use crate::utils::task::resolve_id_or_slug;
use anyhow::Result;

/// Start a task's session; optionally attach. Fails if already started.
///
/// Performs the same preparation as `attach` (ensure branch/worktree, compute agent cmd),
/// then optionally attaches to the daemon sending `OpenSession` with the real terminal size.
pub fn run_with_attach(ctx: &AppContext, task_ident: &str, attach: bool) -> Result<()> {
  // Resolve task
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
  // Fail when a session is already running for this task
  let existing = get_project_state(ctx)?
    .sessions
    .into_iter()
    .any(|e| e.task.id == task.id && e.task.slug == task.slug);
  if existing {
    anyhow::bail!("Already started. Use attach");
  }
  let plan = build_session_plan(ctx, &task)?;
  let bootstrap_request = plan.bootstrap_request.clone();

  crate::utils::daemon::notify_after_task_change(ctx, || {
    // Send bootstrap request to daemon BEFORE starting session
    // This ensures fast bootstrap commands complete before the agent starts
    if let Some(request) = bootstrap_request {
      send_start_bootstrap(ctx, request);
    }

    // Start session (creates tmux session and sends agent command)
    start_session_for_task(ctx, &plan, false)?;

    // Attach if requested (this blocks until user detaches)
    if attach {
      crate::utils::interactive::scope(|| {
        crate::utils::tmux::attach_session(&ctx.config, &plan.task_meta)
      })
    } else {
      Ok(())
    }
  })
}
