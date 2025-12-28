use anyhow::Result;

use crate::config::AppContext;
use crate::utils::files::{file_path, files_dir_for_task, resolve_file};
use crate::utils::task::resolve_id_or_slug;

pub fn run(ctx: &AppContext, task_ident: &str, file_ident: Option<&str>) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;

  let path = if let Some(file_ident) = file_ident {
    let file = resolve_file(&ctx.paths, &task, file_ident)?;
    file_path(&ctx.paths, &task, &file)
  } else {
    files_dir_for_task(&ctx.paths, &task)
  };

  println!("{}", path.display());
  Ok(())
}
