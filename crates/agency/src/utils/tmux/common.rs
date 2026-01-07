use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::AgencyConfig;

pub const GUARD_SESSION: &str = "__agency_guard__";
pub const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(2);

/// Get the tmux socket path from config or environment.
pub fn tmux_socket_path(cfg: &AgencyConfig) -> PathBuf {
  if let Ok(env_path) = std::env::var("AGENCY_TMUX_SOCKET_PATH") {
    return PathBuf::from(env_path);
  }
  if let Some(ref daemon) = cfg.daemon
    && let Some(ref p) = daemon.tmux_socket_path
  {
    return PathBuf::from(p);
  }
  // Default: $XDG_RUNTIME_DIR/agency-tmux.sock or ~/.local/run/agency-tmux.sock
  if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
    return PathBuf::from(xdg_runtime).join("agency-tmux.sock");
  }
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  PathBuf::from(home).join(".local/run/agency-tmux.sock")
}

/// Get base tmux command arguments with socket path.
pub fn tmux_args_base(cfg: &AgencyConfig) -> Vec<String> {
  let sock = tmux_socket_path(cfg);
  vec!["-S".to_string(), sock.display().to_string()]
}

/// Run a command and return an error if it fails.
///
/// # Errors
/// Returns an error if the command fails to spawn or exits with non-zero status.
pub fn run_cmd(cmd: &mut std::process::Command) -> Result<()> {
  let status = cmd.status().with_context(|| format!("spawn {cmd:?}"))?;
  if status.success() {
    Ok(())
  } else {
    anyhow::bail!("command failed: {cmd:?}")
  }
}

/// Escape a path for use in shell commands.
pub fn shell_escape(path: &Path) -> String {
  path
    .display()
    .to_string()
    .replace('\\', "\\\\")
    .replace('"', "\\\"")
    .replace('\'', "'\\''")
}
