use anyhow::Result;

use crate::config::AppContext;
// Use macro via module path
use crate::log_info;
use crate::utils::editor::open_path;
use crate::utils::log::t;
use crate::utils::task::{resolve_id_or_slug, worktree_dir};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let wt_dir = worktree_dir(&ctx.paths, &tref);
  log_info!("Open worktree {}", t::path(wt_dir.display()));
  open_path(&wt_dir)
}
