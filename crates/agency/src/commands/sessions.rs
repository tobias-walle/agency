use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::term::print_table;

pub fn run(ctx: &AppContext) -> Result<()> {
  let state = get_project_state(ctx)?;
  let headers = ["SESSION", "TASK", "CLIENTS", "STATUS", "CWD"];
  let rows: Vec<Vec<String>> = state
    .sessions
    .into_iter()
    .map(|e| {
      vec![
        e.session_id.to_string(),
        format!("{}-{}", e.task.id, e.task.slug),
        e.clients.to_string(),
        e.status,
        e.cwd,
      ]
    })
    .collect();
  print_table(&headers, &rows);
  Ok(())
}
