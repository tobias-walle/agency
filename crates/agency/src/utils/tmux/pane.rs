use anyhow::{Context, Result};

use crate::config::AgencyConfig;

use super::common::{run_cmd, tmux_args_base};

/// Send keys to a tmux pane.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn send_keys(cfg: &AgencyConfig, target: &str, text: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("send-keys")
      .arg("-t")
      .arg(target)
      .arg(text),
  )
}

/// Send Enter key to a tmux pane.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn send_keys_enter(cfg: &AgencyConfig, target: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("send-keys")
      .arg("-t")
      .arg(target)
      .arg("Enter"),
  )
}

/// Check if a pane is dead.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn pane_dead(cfg: &AgencyConfig, target: &str) -> Result<bool> {
  let out = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("list-panes")
    .arg("-F")
    .arg("#{pane_dead}")
    .arg("-t")
    .arg(target)
    .output()
    .context("tmux list-panes failed")?;
  if !out.status.success() {
    return Ok(false);
  }
  let s = String::from_utf8_lossy(&out.stdout);
  Ok(s.lines().any(|l| l.trim() == "1"))
}
