use std::collections::HashMap;

use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::task::{TaskRef, list_tasks};
use crate::utils::task_columns::{GitMetrics, TaskColumn, TaskRow};
use crate::utils::term::print_table;

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  tasks.sort_by_key(|t| t.id);

  // Query project state (sessions + metrics); fallback gracefully when daemon unavailable
  let (sessions, git_metrics_map) = match get_project_state(ctx) {
    Ok(state) => {
      let m: HashMap<TaskRef, GitMetrics> = state
        .metrics
        .into_iter()
        .map(|m| {
          (
            TaskRef::from(m.task),
            GitMetrics {
              uncommitted_add: m.uncommitted_add,
              uncommitted_del: m.uncommitted_del,
              commits_ahead: m.commits_ahead,
            },
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
      let git_metrics = git_metrics_map.get(t).cloned().unwrap_or_default();
      TaskRow::new(ctx, t.clone(), latest.get(t), git_metrics)
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
  use crate::utils::task::TaskRef;

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
    let map = latest_sessions_by_task(&[s1.clone(), s2.clone(), s3_other.clone()]);
    let task_a = TaskRef { id: 5, slug: "a".to_string() };
    let task_b = TaskRef { id: 6, slug: "b".to_string() };
    let a_latest = map.get(&task_a).expect("has a");
    assert_eq!(a_latest.session_id, 2);
    let b_latest = map.get(&task_b).expect("has b");
    assert_eq!(b_latest.session_id, 10);
  }
}
