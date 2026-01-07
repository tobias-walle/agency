use anyhow::{Result, bail};

use crate::config::AppContext;
use crate::utils::context::is_in_worktree;
use crate::utils::files::{display_path, list_files};
use crate::utils::fzf::{parse_id_from_selection, run_fzf};
use crate::utils::task::resolve_id_or_slug;

pub fn run(ctx: &AppContext, task_ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
  let files = list_files(&ctx.paths, &task)?;

  if files.is_empty() {
    bail!("No files attached to task");
  }

  let in_worktree = is_in_worktree(&ctx.paths);

  let lines: Vec<String> = files
    .iter()
    .map(|f| {
      let path = display_path(&ctx.paths, &task, f, in_worktree);
      format!("{}\t{}\t{}", f.id, f.name, path)
    })
    .collect();

  let input = lines.join("\n");
  let selected = run_fzf(&input)?;
  let id = parse_id_from_selection(selected, "file")?;

  println!("{id}");
  Ok(())
}
