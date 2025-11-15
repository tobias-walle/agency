use anyhow::Result;

use crate::config::AppContext;
use crate::daemon_protocol::TaskMeta;
use crate::utils::daemon::get_project_state;
use crate::utils::interactive;
use crate::utils::session::{build_session_plan, start_session_for_task};
use crate::utils::task::resolve_id_or_slug;
use crate::utils::tmux;

pub fn run_with_task(ctx: &AppContext, ident: &str) -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Resolve task
  let task = resolve_id_or_slug(&ctx.paths, ident)?;

  // Query existing sessions and join the latest for this task; error if none
  let entries = get_project_state(ctx)?.sessions;
  let target = entries
    .into_iter()
    .filter(|e| e.task.id == task.id && e.task.slug == task.slug)
    .max_by_key(|e| e.created_at_ms);

  let task_meta = TaskMeta {
    id: task.id,
    slug: task.slug.clone(),
  };
  if target.is_some() {
    return interactive::scope(|| tmux::attach_session(&ctx.config, &task_meta));
  }
  // Auto-start when missing using shared session helpers, then attach
  let plan = build_session_plan(ctx, &task)?;
  crate::utils::daemon::notify_after_task_change(ctx, || start_session_for_task(ctx, &plan, true))
}

pub fn run_join_session(ctx: &AppContext, session_id: u64) -> Result<()> {
  let entries = get_project_state(ctx)?.sessions;
  let Some(si) = entries.into_iter().find(|e| e.session_id == session_id) else {
    anyhow::bail!("Session not found: {session_id}");
  };
  interactive::scope(|| tmux::attach_session(&ctx.config, &si.task))
}
