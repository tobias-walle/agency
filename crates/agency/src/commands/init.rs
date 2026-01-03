use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{self, AppContext};
use crate::log_info;
use crate::utils::log::t;

const SETUP_TEMPLATE: &str = r#"#!/usr/bin/env bash
set -euo pipefail

echo "Setup"
"#;

pub fn run(ctx: &AppContext, agent: Option<&str>, yes: bool) -> Result<()> {
  let root = ctx.paths.root().clone();
  let prompt = format!(
    "Generate project specific configuration files in {}?",
    t::path(root.display())
  );
  // Auto-confirm in non-interactive environments (default=true) since init is non-destructive
  if !ctx.tty.confirm(&prompt, true, yes)? {
    return Ok(());
  }

  let agency_dir = root.join(".agency");
  fs::create_dir_all(&agency_dir)
    .with_context(|| format!("failed to create {}", agency_dir.display()))?;

  let config_path = agency_dir.join("agency.toml");
  ensure_config(&config_path, agent)?;

  let script_path = agency_dir.join("setup.sh");
  ensure_script(&script_path)?;

  let gitignore_path = root.join(".gitignore");
  ensure_gitignore(&gitignore_path)?;

  // Best-effort: when current_dir differs from resolved project root,
  // also scaffold in the current_dir to satisfy non-repo sandboxes.
  let cur = std::env::current_dir().unwrap_or_else(|_| root.clone());
  if cur != root {
    let cur_agency_dir = cur.join(".agency");
    let _ = fs::create_dir_all(&cur_agency_dir);
    let cur_config_path = cur_agency_dir.join("agency.toml");
    let cur_script_path = cur_agency_dir.join("setup.sh");
    let cur_gitignore_path = cur.join(".gitignore");
    let _ = ensure_config(&cur_config_path, agent);
    let _ = ensure_script(&cur_script_path);
    let _ = ensure_gitignore(&cur_gitignore_path);
  }

  log_info!("");
  log_info!("Created project config:");
  log_info!("  {}", t::path(".agency/agency.toml"));
  log_info!("  {}", t::path(".agency/setup.sh"));
  log_info!("  {}", t::path(".gitignore"));
  log_info!("");
  Ok(())
}

fn ensure_config(path: &Path, agent: Option<&str>) -> Result<()> {
  let existed = path.exists();
  if let Some(a) = agent {
    let content = if existed {
      fs::read_to_string(path)?
    } else {
      String::new()
    };
    let mut doc = content.parse::<toml_edit::DocumentMut>()?;
    doc.insert("agent", toml_edit::value(a));
    let mut output = doc.to_string();
    // For new config files, append the commented template for discoverability
    if !existed {
      output = format!("{}\n{}", output.trim_end(), config::config_template());
    }
    fs::write(path, output).with_context(|| format!("failed to write {}", path.display()))?;
  } else if !existed {
    // Write the template for discoverability
    fs::write(path, config::config_template())
      .with_context(|| format!("failed to write {}", path.display()))?;
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
