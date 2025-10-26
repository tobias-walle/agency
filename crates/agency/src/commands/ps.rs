use anyhow::Result;

use crate::config::AgencyConfig;
use crate::utils::task::list_tasks;
use crate::utils::term::print_table;

pub fn run(cfg: &AgencyConfig) -> Result<()> {
  let mut tasks = list_tasks(cfg)?;
  tasks.sort_by_key(|t| t.id);

  let rows: Vec<Vec<String>> = tasks
    .iter()
    .map(|t| vec![t.id.to_string(), t.slug.clone()])
    .collect();

  print_table(&["ID", "SLUG"], &rows);

  Ok(())
}
