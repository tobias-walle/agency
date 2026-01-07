use anstream::println;
use anyhow::Result;

use crate::config::AppContext;
use crate::utils::task::{resolve_id_or_slug, worktree_dir};

pub fn run(ctx: &AppContext, task_ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, task_ident)?;
  let dir = worktree_dir(&ctx.paths, &tref);
  println!("{}", dir.display());
  Ok(())
}
