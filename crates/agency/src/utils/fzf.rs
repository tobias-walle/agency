use std::io::Write as _;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::utils::which;

pub(crate) const FZF_NOT_INSTALLED_ERROR: &str =
  "fzf is not installed. Install it from https://github.com/junegunn/fzf";

/// Runs fzf with the given input and returns the selected line, or None if cancelled.
///
/// # Errors
/// Returns an error if fzf is not installed or fails to spawn or encounters an I/O error.
pub(crate) fn run_fzf(input: &str) -> Result<Option<String>> {
  if which::which("fzf").is_none() {
    bail!(FZF_NOT_INSTALLED_ERROR);
  }

  let mut child = Command::new("fzf")
    .args(["--no-multi", "--height=~50%"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;

  if let Some(stdin) = child.stdin.as_mut() {
    stdin.write_all(input.as_bytes())?;
  }

  let output = child.wait_with_output()?;

  if !output.status.success() {
    return Ok(None);
  }

  let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
  if selected.is_empty() {
    return Ok(None);
  }

  Ok(Some(selected))
}

/// Parses an ID from a tab-separated fzf selection where the first column is the ID.
/// If selection is None, exits the process with code 1 (user cancelled).
///
/// # Errors
/// Returns an error if the ID cannot be parsed from the selection.
pub(crate) fn parse_id_from_selection(selected: Option<String>, entity_type: &str) -> Result<u32> {
  let Some(selected) = selected else {
    std::process::exit(1);
  };

  let id = selected
    .split('\t')
    .next()
    .and_then(|s| s.parse::<u32>().ok());

  let Some(id) = id else {
    bail!("Failed to parse {entity_type} ID from selection");
  };

  Ok(id)
}
