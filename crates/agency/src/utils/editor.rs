use std::path::Path;
use std::process::Command;

use crate::utils::interactive;
use anyhow::{Context, Result, bail};

use crate::config::AgencyConfig;
use super::error_messages;

/// Open a file or directory path using the configured editor.
/// Falls back to $EDITOR or `vi` when not configured.
pub(crate) fn open_path(cfg: &AgencyConfig, path: &Path, cwd: &Path) -> Result<()> {
  let target = path
    .canonicalize()
    .unwrap_or_else(|_| path.to_path_buf())
    .display()
    .to_string();

  let editor_argv = cfg.editor_argv();
  let (program, rest) = editor_argv
    .split_first()
    .ok_or_else(|| anyhow::anyhow!("invalid editor argv: empty"))?;
  let mut launch_args: Vec<String> = rest.to_vec();
  launch_args.push(target);

  interactive::scope(|| {
    let status = Command::new(program)
      .args(&launch_args)
      .current_dir(cwd)
      .status()
      .with_context(|| format!("failed to spawn editor program: {program}"))?;
    if !status.success() {
      bail!(error_messages::EDITOR_NON_ZERO_EXIT);
    }
    Ok(())
  })
}
