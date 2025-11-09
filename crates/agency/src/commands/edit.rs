use anyhow::Result;

use crate::config::AppContext;
use crate::log_info;
use crate::utils::editor::open_path;
use crate::utils::log::t;
use crate::utils::task::{resolve_id_or_slug, task_file};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let tf = task_file(&ctx.paths, &tref);
  log_info!("Edit markdown {}", t::path(tf.display()));
  open_path(&tf)
}
