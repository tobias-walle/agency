use anyhow::{Context, Result};

use crate::config::{self, AppContext};
use crate::utils::editor::open_path;

pub fn run(ctx: &AppContext) -> Result<()> {
  let cfg_path = config::global_config_path()?;
  if let Some(parent) = cfg_path.parent() {
    std::fs::create_dir_all(parent)
      .with_context(|| format!("failed to create {}", parent.display()))?;
  }
  if !cfg_path.exists() {
    // Create an empty config file by default if missing
    std::fs::write(&cfg_path, b"")
      .with_context(|| format!("failed to create {}", cfg_path.display()))?;
  }
  open_path(&ctx.config, &cfg_path, ctx.paths.root())
}
