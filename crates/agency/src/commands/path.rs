use anyhow::Result;

use crate::config::AgencyConfig;
use crate::utils::task::{resolve_id_or_slug, worktree_dir};

pub fn run(cfg: &AgencyConfig, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(cfg, ident)?;
  let dir = worktree_dir(cfg, &tref);
  anstream::println!("{}", dir.display());
  Ok(())
}
