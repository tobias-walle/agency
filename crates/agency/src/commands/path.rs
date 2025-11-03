use anyhow::Result;

use crate::config::AppContext;
use crate::utils::task::{resolve_id_or_slug, worktree_dir};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let dir = worktree_dir(&ctx.paths, &tref);
  anstream::println!("{}", dir.display());
  Ok(())
}
