use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::config::AgencyConfig;

use super::common::{tmux_args_base, tmux_socket_path, GUARD_SESSION, SERVER_READY_TIMEOUT};

/// Ensure the socket directory exists with proper permissions (0700).
///
/// # Errors
/// Returns an error if the directory cannot be created or permissions cannot be set.
fn ensure_socket_directory(cfg: &AgencyConfig) -> Result<()> {
  use std::os::unix::fs::PermissionsExt;

  let sock = tmux_socket_path(cfg);
  let Some(dir) = sock.parent() else {
    return Ok(());
  };

  std::fs::create_dir_all(dir)
    .with_context(|| format!("failed to create socket directory: {}", dir.display()))?;

  std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    .with_context(|| format!("failed to set permissions on socket directory: {}", dir.display()))?;

  Ok(())
}

/// Remove stale socket file if it exists but the server isn't running.
/// This is best-effort and won't fail if something goes wrong.
fn cleanup_stale_socket(cfg: &AgencyConfig) {
  let sock = tmux_socket_path(cfg);
  if !sock.exists() {
    return;
  }

  // Try to check if server is responsive
  let responsive = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("list-sessions")
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .is_ok_and(|st| st.success());

  if !responsive {
    // Socket exists but server isn't responding - remove stale socket
    let _ = std::fs::remove_file(&sock);
  }
}

/// Wait for the tmux server to become responsive.
///
/// # Errors
/// Returns an error with captured stderr if the server doesn't respond within the timeout.
fn wait_for_server_ready(cfg: &AgencyConfig, timeout: Duration) -> Result<()> {
  let start = Instant::now();
  let mut delay_ms = 10u64;
  let max_delay_ms = 200u64;
  let mut last_stderr = String::new();

  while start.elapsed() < timeout {
    let output = std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("list-sessions")
      .stdout(std::process::Stdio::null())
      .output();

    match output {
      Ok(out) if out.status.success() => return Ok(()),
      Ok(out) => {
        last_stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
      }
      Err(err) => {
        last_stderr = err.to_string();
      }
    }

    std::thread::sleep(Duration::from_millis(delay_ms));
    delay_ms = (delay_ms * 2).min(max_delay_ms);
  }

  if last_stderr.is_empty() {
    anyhow::bail!("tmux server did not become ready within {timeout:?}");
  }
  anyhow::bail!("tmux server did not become ready within {timeout:?}: {last_stderr}");
}

fn guard_session_exited_immediately(cfg: &AgencyConfig) -> Result<()> {
  let sock = tmux_socket_path(cfg);
  anyhow::bail!(
    "tmux guard session exited immediately after start. \
This usually indicates a tmux configuration issue. \
Try running `tmux -S {}` manually or temporarily disabling your tmux config.",
    sock.display()
  );
}

/// Ensure a dedicated tmux server is running on our socket by maintaining a
/// hidden guard session.
///
/// # Errors
/// Returns an error if the socket directory cannot be created or the server fails to start.
pub fn ensure_server(cfg: &AgencyConfig) -> Result<()> {
  // Create socket directory with proper permissions
  ensure_socket_directory(cfg)?;

  // Clean up stale socket if present
  cleanup_stale_socket(cfg);

  // If guard session exists, server is already up
  if is_server_running(cfg) {
    return Ok(());
  }

  // Create a detached guard session (starts server if needed)
  let output = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("new-session")
    .arg("-d")
    .arg("-s")
    .arg(GUARD_SESSION)
    .output()
    .context("failed to spawn tmux new-session")?;

  if !output.status.success() {
    // If new-session failed, recheck if guard exists (race condition)
    if is_server_running(cfg) {
      return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("failed to create tmux guard session: {}", stderr.trim());
  }

  if !is_server_running(cfg) {
    return guard_session_exited_immediately(cfg);
  }

  // Wait for the server to become responsive
  wait_for_server_ready(cfg, SERVER_READY_TIMEOUT)
}

/// Ensure tmux server is running and allow tmux stderr to pass through
/// to the parent process. This is intended for explicit daemon start/restart
/// commands to aid debugging, while other code paths keep tmux output hidden.
///
/// # Errors
/// Returns an error if the socket directory cannot be created or the server fails to start.
pub fn ensure_server_inherit_stderr(cfg: &AgencyConfig) -> Result<()> {
  ensure_socket_directory(cfg)?;
  cleanup_stale_socket(cfg);

  if is_server_running(cfg) {
    return Ok(());
  }

  let new_session_status = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("new-session")
    .arg("-d")
    .arg("-s")
    .arg(GUARD_SESSION)
    .stdout(std::process::Stdio::null())
    .status()
    .context("failed to spawn tmux new-session")?;
  if !new_session_status.success() {
    if is_server_running(cfg) {
      return Ok(());
    }
    anyhow::bail!("failed to create tmux guard session");
  }

  if !is_server_running(cfg) {
    return guard_session_exited_immediately(cfg);
  }

  let start = Instant::now();
  let mut delay_ms = 10u64;
  let max_delay_ms = 200u64;
  while start.elapsed() < SERVER_READY_TIMEOUT {
    let ready_status = std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("list-sessions")
      .stdout(std::process::Stdio::null())
      .status();
    if let Ok(status) = ready_status && status.success() {
      return Ok(());
    }
    std::thread::sleep(Duration::from_millis(delay_ms));
    delay_ms = (delay_ms * 2).min(max_delay_ms);
  }

  anyhow::bail!("tmux server did not become ready within {SERVER_READY_TIMEOUT:?}")
}

/// Check if the tmux server is running by checking for the guard session.
pub fn is_server_running(cfg: &AgencyConfig) -> bool {
  std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("has-session")
    .arg("-t")
    .arg(GUARD_SESSION)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .is_ok_and(|st| st.success())
}

/// Stop the tmux server on our socket.
///
/// # Errors
/// Returns an error if the server fails to stop (when `wait` is true and server still responds).
pub fn stop_server(cfg: &AgencyConfig, wait: bool) -> Result<()> {
  let output = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("kill-server")
    .output()
    .context("failed to run tmux kill-server")?;

  // Ignore "no server running" errors - that's fine
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.contains("no server running") {
      anyhow::bail!("tmux kill-server failed: {}", stderr.trim());
    }
  }

  if !wait {
    return Ok(());
  }

  // Wait for server to stop responding (more reliable than checking socket file)
  let start = Instant::now();
  let timeout = Duration::from_secs(5);

  while start.elapsed() < timeout {
    // Check if server is still responding
    let still_running = std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("list-sessions")
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .status()
      .is_ok_and(|st| st.success());

    if !still_running {
      // Server is down - clean up stale socket if present
      let sock = tmux_socket_path(cfg);
      let _ = std::fs::remove_file(&sock);
      return Ok(());
    }
    std::thread::sleep(Duration::from_millis(50));
  }

  anyhow::bail!("tmux server still running after kill-server")
}
