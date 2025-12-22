use std::collections::HashMap;

use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::task::list_tasks;
use crate::utils::task_columns::{TaskColumn, TaskRow};
use crate::utils::term::print_table;

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  tasks.sort_by_key(|t| t.id);

  // Query project state (sessions + metrics); fallback gracefully when daemon unavailable
  let (sessions, metrics_map) = match get_project_state(ctx) {
    Ok(state) => {
      let m: HashMap<(u32, String), (u64, u64, u64)> = state
        .metrics
        .into_iter()
        .map(|m| {
          (
            (m.task.id, m.task.slug),
            (m.uncommitted_add, m.uncommitted_del, m.commits_ahead),
          )
        })
        .collect();
      (state.sessions, m)
    }
    Err(_) => (Vec::new(), HashMap::new()),
  };
  let latest = latest_sessions_by_task(&sessions);

  // Build TaskRow structs using the shared constructor
  let task_rows: Vec<TaskRow> = tasks
    .iter()
    .map(|t| {
      let key = (t.id, t.slug.clone());
      let (add, del, ahead) = metrics_map.get(&key).copied().unwrap_or((0, 0, 0));
      TaskRow::new(ctx, t.clone(), latest.get(&key), add, del, ahead)
    })
    .collect();

  // Use TaskColumn to generate headers and cell values
  let headers: Vec<&str> = TaskColumn::ALL.iter().copied().map(TaskColumn::header).collect();
  let rows: Vec<Vec<String>> = task_rows
    .iter()
    .map(|row| {
      TaskColumn::ALL
        .iter()
        .map(|col| col.cell(row, false))
        .collect()
    })
    .collect();

  print_table(&headers, &rows);

  Ok(())
}

#[cfg(test)]
mod tests {
  use crate::daemon_protocol::SessionInfo;
  use crate::utils::sessions::latest_sessions_by_task;

  #[test]
  fn latest_session_selection_by_created_at() {
    let base_a = SessionInfo {
      task: crate::daemon_protocol::TaskMeta {
        id: 5,
        slug: "a".to_string(),
      },
      ..Default::default()
    };
    let s1 = SessionInfo {
      session_id: 1,
      created_at_ms: 100,
      ..base_a.clone()
    };
    let s2 = SessionInfo {
      session_id: 2,
      created_at_ms: 200,
      ..base_a.clone()
    };
    let s3_other = SessionInfo {
      session_id: 10,
      task: crate::daemon_protocol::TaskMeta {
        id: 6,
        slug: "b".to_string(),
      },
      created_at_ms: 150,
      ..Default::default()
    };
    let map = latest_sessions_by_task(&vec![s1.clone(), s2.clone(), s3_other.clone()]);
    let a_latest = map.get(&(5_u32, "a".to_string())).expect("has a");
    assert_eq!(a_latest.session_id, 2);
    let b_latest = map.get(&(6_u32, "b".to_string())).expect("has b");
    assert_eq!(b_latest.session_id, 10);
  }
}
