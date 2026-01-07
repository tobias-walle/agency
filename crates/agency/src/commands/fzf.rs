use std::collections::HashMap;

use anyhow::{Result, bail};

use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::fzf::{parse_id_from_selection, run_fzf};
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::status::derive_status;
use crate::utils::task::{TaskRef, list_tasks, worktree_dir};

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  if tasks.is_empty() {
    bail!("No tasks found");
  }
  tasks.sort_by_key(|t| t.id);

  let (sessions, wt_exists_map) = get_task_state(ctx, &tasks);
  let latest = latest_sessions_by_task(&sessions);

  let lines: Vec<String> = tasks
    .iter()
    .map(|t| {
      let wt_exists = wt_exists_map.get(t).copied().unwrap_or(false);
      let status = derive_status(latest.get(t), wt_exists);
      format!("{}\t{}\t{}", t.id, t.slug, status.label())
    })
    .collect();

  let input = lines.join("\n");
  let selected = run_fzf(&input)?;
  let id = parse_id_from_selection(selected, "task")?;

  println!("{id}");
  Ok(())
}

fn get_task_state(
  ctx: &AppContext,
  tasks: &[TaskRef],
) -> (Vec<crate::daemon_protocol::SessionInfo>, HashMap<TaskRef, bool>) {
  let sessions = match get_project_state(ctx) {
    Ok(state) => state.sessions,
    Err(_) => Vec::new(),
  };

  let wt_exists_map: HashMap<TaskRef, bool> = tasks
    .iter()
    .map(|t| (t.clone(), worktree_dir(&ctx.paths, t).exists()))
    .collect();

  (sessions, wt_exists_map)
}
