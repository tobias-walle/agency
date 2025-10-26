use anyhow::Result;
use std::io::{self, IsTerminal as _};
use owo_colors::OwoColorize as _;

use crate::config::AgencyConfig;
use crate::utils::task::list_tasks;
use crate::utils::term::print_table;

pub fn run(cfg: &AgencyConfig) -> Result<()> {
  let mut tasks = list_tasks(cfg)?;
  tasks.sort_by_key(|t| t.id);

  let color_ids = io::stdout().is_terminal();
  let rows: Vec<Vec<String>> = tasks
    .iter()
    .map(|t| {
      let id = if color_ids { format!("{}", t.id.to_string().cyan()) } else { t.id.to_string() };
      vec![id, t.slug.clone()]
    })
    .collect();

  print_table(&["ID", "SLUG"], &rows);

  Ok(())
}
