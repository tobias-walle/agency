use anyhow::Result;

use crate::config::AppContext;
use crate::log_info;
use crate::utils::files::{file_path, files_dir_for_task, resolve_file};
use crate::utils::log::t;
use crate::utils::opener::open_with_default;
use crate::utils::task::resolve_id_or_slug;

pub fn run(ctx: &AppContext, task_ident: &str, file_ident: Option<&str>) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;

  let path = if let Some(ident) = file_ident {
    let file = resolve_file(&ctx.paths, &task, ident)?;
    log_info!("Open file {} {}", t::id(file.id), t::path(&file.name));
    file_path(&ctx.paths, &task, &file)
  } else {
    let dir = files_dir_for_task(&ctx.paths, &task);
    log_info!("Open files directory {}", t::path(dir.display()));
    dir
  };

  open_with_default(&path)
}
