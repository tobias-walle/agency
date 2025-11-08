use anyhow::Result;
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::utils::editor::open_path;
use crate::utils::task::{resolve_id_or_slug, worktree_dir};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let wt_dir = worktree_dir(&ctx.paths, &tref);
  anstream::println!("Open worktree {}", wt_dir.display().to_string().cyan());
  open_path(&wt_dir)
}
