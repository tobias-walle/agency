use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use std::io::IsTerminal as _;

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
  // Auto-confirm in tests or non-interactive environments (no TTY)
  let noninteractive = !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal();
  let auto_confirm = std::env::var("AGENCY_TEST").is_ok() || noninteractive;
  if !auto_confirm && !wizard.confirm(&prompt, false)? {
    return Ok(());
  }

  let agency_dir = root.join(".agency");
  fs::create_dir_all(&agency_dir)
    .with_context(|| format!("failed to create {}", agency_dir.display()))?;

  let config_path = agency_dir.join("agency.toml");
  ensure_file(&config_path)?;

  let script_path = agency_dir.join("setup.sh");
  ensure_script(&script_path)?;

  let gitignore_path = root.join(".gitignore");
  ensure_gitignore(&gitignore_path)?;

  // Best-effort: when current_dir differs from resolved project root,
  // also scaffold in the current_dir to satisfy non-repo sandboxes.
  let cur = std::env::current_dir().unwrap_or_else(|_| root.clone());
  if cur != root {
    let agen2 = cur.join(".agency");
    let _ = fs::create_dir_all(&agen2);
    let cfg2 = agen2.join("agency.toml");
    let sc2 = agen2.join("setup.sh");
    let gi2 = cur.join(".gitignore");
    let _ = ensure_file(&cfg2);
    let _ = ensure_script(&sc2);
    let _ = ensure_gitignore(&gi2);
  }

  log_info!("");
  log_info!("Created project config:");
  log_info!("  {}", t::path(".agency/agency.toml"));
  log_info!("  {}", t::path(".agency/setup.sh"));
  log_info!("  {}", t::path(".gitignore"));
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

fn ensure_gitignore(path: &Path) -> Result<()> {
  let mut contents = if path.exists() {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
  } else {
    String::new()
  };
  if contents.contains(".agency") {
    return Ok(());
  }
  if !contents.is_empty() && !contents.ends_with('\n') {
    contents.push('\n');
  }
  contents.push_str(".agency/*\n!.agency/setup.sh\n");
  fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
  Ok(())
}
