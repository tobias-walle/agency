use anyhow::Result;

use crate::config::AppContext;
use crate::utils::context::is_in_worktree;
use crate::utils::files::{list_files, print_files_table};
use crate::utils::task::resolve_id_or_slug;

pub fn run(ctx: &AppContext, task_ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
  let files = list_files(&ctx.paths, &task)?;

  if files.is_empty() {
    println!("No files attached.");
    return Ok(());
  }

  let in_worktree = is_in_worktree(&ctx.paths);
  print_files_table(&ctx.paths, &task, &files, in_worktree);

  Ok(())
}
