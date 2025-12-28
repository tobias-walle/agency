use anyhow::Result;

use crate::config::AppContext;
use crate::log_info;
use crate::utils::editor::open_path;
use crate::utils::files::{file_path, resolve_file};
use crate::utils::log::t;
use crate::utils::task::resolve_id_or_slug;

pub fn run(ctx: &AppContext, task_ident: &str, file_ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;
  let file = resolve_file(&ctx.paths, &task, file_ident)?;
  let path = file_path(&ctx.paths, &task, &file);

  log_info!("Edit file {} {}", t::id(file.id), t::path(&file.name));
  open_path(&ctx.config, &path, ctx.paths.root())
}
