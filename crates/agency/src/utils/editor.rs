use std::path::Path;
use std::process::Command;

use crate::utils::interactive;
use anyhow::{Context, Result, bail};

use crate::config::AgencyConfig;

/// Open a file or directory path using the configured editor.
/// Falls back to $EDITOR or `vi` when not configured.
pub fn open_path(cfg: &AgencyConfig, path: &Path, cwd: &Path) -> Result<()> {
  // In test environments, skip actually launching an editor to avoid hangs
  if std::env::var("AGENCY_TEST").is_ok() {
    return Ok(());
  }
  let target = path
    .canonicalize()
    .unwrap_or_else(|_| path.to_path_buf())
    .display()
    .to_string();

  let argv = cfg.editor_argv();
  let (program, rest) = argv
    .split_first()
    .ok_or_else(|| anyhow::anyhow!("invalid editor argv: empty"))?;
  let mut args: Vec<String> = rest.to_vec();
  args.push(target);

  interactive::scope(|| {
    let status = Command::new(program)
      .args(&args)
      .current_dir(cwd)
      .status()
      .with_context(|| format!("failed to spawn editor program: {program}"))?;
    if !status.success() {
      bail!("editor exited with non-zero status");
    }
    Ok(())
  })
}
