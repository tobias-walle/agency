use anyhow::Result;

use crate::config::AppContext;
use crate::utils::task::list_tasks;
use crate::utils::term::print_table;

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  tasks.sort_by_key(|t| t.id);

  let rows: Vec<Vec<String>> = tasks
    .iter()
    .map(|t| vec![t.id.to_string(), t.slug.clone()])
    .collect();

  print_table(&["ID", "SLUG"], &rows);

  Ok(())
}
