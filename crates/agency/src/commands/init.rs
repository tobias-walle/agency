use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::log_info;
use crate::utils::log::t;
use crate::utils::wizard::Wizard;

const SETUP_TEMPLATE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

echo "Setup"
"#;

pub fn run(ctx: &AppContext) -> Result<()> {
  let wizard = Wizard::new();
  let root = ctx.paths.cwd().clone();
  let prompt = format!(
    "Generate project specific configuration files in {}?",
    t::path(root.display())
  );
  if !wizard.confirm(&prompt, false)? {
    return Ok(());
  }

  let agency_dir = root.join(".agency");
  fs::create_dir_all(&agency_dir)
    .with_context(|| format!("failed to create {}", agency_dir.display()))?;

  let config_path = agency_dir.join("agency.toml");
  ensure_file(&config_path)?;

  let script_path = agency_dir.join("setup.sh");
  ensure_script(&script_path)?;

  log_info!("");
  log_info!("Created project config:");
  log_info!("  {}", t::path(".agency/agency.toml"));
  log_info!("  {}", t::path(".agency/setup.sh"));
  log_info!("");
  Ok(())
}

fn ensure_file(path: &Path) -> Result<()> {
  if !path.exists() {
    fs::write(path, b"").with_context(|| format!("failed to write {}", path.display()))?;
  }
  Ok(())
}

fn ensure_script(path: &Path) -> Result<()> {
  if !path.exists() {
    fs::write(path, SETUP_TEMPLATE)
      .with_context(|| format!("failed to write {}", path.display()))?;
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
      .with_context(|| format!("failed to set exec bit on {}", path.display()))?;
  }
  Ok(())
}
