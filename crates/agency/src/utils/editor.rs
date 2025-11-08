use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Open a file or directory path using the user's `$EDITOR`.
/// Parses `$EDITOR` via shell-words and appends the canonicalized path.
pub fn open_path(path: &Path) -> Result<()> {
  let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
  let target = path
    .canonicalize()
    .unwrap_or_else(|_| path.to_path_buf())
    .display()
    .to_string();

  let tokens = shell_words::split(&editor).context("invalid $EDITOR value")?;
  if tokens.is_empty() {
    bail!("invalid $EDITOR value: empty");
  }
  let program = &tokens[0];
  let mut args: Vec<String> = tokens[1..].to_vec();
  args.push(target);

  let status = Command::new(program)
    .args(&args)
    .status()
    .with_context(|| format!("failed to spawn editor program: {program}"))?;
  if !status.success() {
    bail!("editor exited with non-zero status");
  }
  Ok(())
}
