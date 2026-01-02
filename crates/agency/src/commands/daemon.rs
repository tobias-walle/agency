use std::os::unix::net::UnixStream;
use std::process::Command as ProcCommand;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use log::{info, warn};

use crate::config::{compute_socket_path, load_config};
use crate::daemon as slim_daemon;
use crate::daemon_protocol::{C2D, C2DControl, write_frame};
use crate::utils::daemon::connect_daemon_socket;
use crate::utils::tmux;
use crate::AppContext;

pub fn run_blocking() -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Compute socket path from config (project + XDG)
  let cwd = std::env::current_dir()?;
  let cfg = load_config(&cwd)?;
  let socket = compute_socket_path(&cfg);

  slim_daemon::run_daemon(&socket, &cfg)
}

pub fn start() -> Result<()> {
  let cwd = std::env::current_dir()?;
  let cfg = load_config(&cwd)?;
  let socket = compute_socket_path(&cfg);

  if UnixStream::connect(&socket).is_ok() {
    warn!("Daemon already running");
    return Ok(());
  }

  // Spawn detached child running `agency daemon run`
  let exe = std::env::current_exe().context("failed to get current exe")?;
  let mut daemon_cmd = ProcCommand::new(exe);
  daemon_cmd.arg("daemon").arg("run");
  daemon_cmd.stdout(std::process::Stdio::null());
  daemon_cmd.stderr(std::process::Stdio::null());
  daemon_cmd.stdin(std::process::Stdio::null());
  let _child = daemon_cmd.spawn().context("failed to spawn daemon child")?;

  // Poll for readiness
  let poll_start = Instant::now();
  while poll_start.elapsed() < Duration::from_secs(5) {
    if std::fs::metadata(&socket).is_ok() {
      info!("Started daemon at {}", socket.display());
      // Ensure tmux server is running
      tmux::ensure_server(&cfg)?;
      return Ok(());
    }
    thread::sleep(Duration::from_millis(50));
  }
  anyhow::bail!(
    "Daemon did not create socket at {} within timeout",
    socket.display()
  )
}

/// Stop the daemon, optionally stopping tmux server as well.
///
/// # Errors
/// Returns an error if daemon fails to stop.
pub fn stop(ctx: &AppContext, yes: bool) -> Result<()> {
  let socket = compute_socket_path(&ctx.config);

  // Check if tmux is running and ask about stopping it
  let tmux_running = tmux::is_server_running(&ctx.config);
  let stop_tmux = if tmux_running {
    ctx
      .tty
      .confirm("Also stop tmux server? This will terminate all running tasks", false, yes)?
  } else {
    false
  };

  // Stop daemon
  if let Ok(mut stream) = connect_daemon_socket(&socket) {
    let _ = write_frame(&mut stream, &C2D::Control(C2DControl::Shutdown));
    let _ = stream.shutdown(std::net::Shutdown::Both);

    // Wait for the socket file to disappear
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
      if std::fs::metadata(&socket).is_err() {
        info!("Stopped daemon");
        break;
      }
      thread::sleep(Duration::from_millis(50));
    }
  }

  // Stop tmux if confirmed
  if stop_tmux {
    tmux::stop_server(&ctx.config, true)?;
    info!("Stopped tmux server");
  }

  Ok(())
}

/// Internal function to stop daemon only (no tmux, no prompts).
fn stop_daemon_only() -> Result<()> {
  let cwd = std::env::current_dir()?;
  let cfg = load_config(&cwd)?;
  let socket = compute_socket_path(&cfg);

  let mut stream = connect_daemon_socket(&socket)?;
  write_frame(&mut stream, &C2D::Control(C2DControl::Shutdown))
    .context("failed to send Shutdown frame")?;
  let _ = stream.shutdown(std::net::Shutdown::Both);

  // Wait for the socket file to disappear
  let start = Instant::now();
  while start.elapsed() < Duration::from_secs(5) {
    if std::fs::metadata(&socket).is_err() {
      info!("Stopped daemon");
      return Ok(());
    }
    thread::sleep(Duration::from_millis(50));
  }
  anyhow::bail!("Daemon socket still present after stop")
}

/// Internal function to restart daemon only, ensuring tmux is running.
/// Used for automatic version-mismatch restarts.
pub fn restart_daemon_only() -> Result<()> {
  let _ = stop_daemon_only();
  start()
}

/// User-facing restart command with optional tmux server restart.
///
/// # Errors
/// Returns an error if daemon or tmux server fails to start.
pub fn restart(ctx: &AppContext, yes: bool) -> Result<()> {
  let tmux_was_running = tmux::is_server_running(&ctx.config);

  // If tmux is running, ask for confirmation before restarting (kills all tasks)
  let restart_tmux = if tmux_was_running {
    ctx
      .tty
      .confirm("Restart tmux server? This will terminate all running tasks", false, yes)?
  } else {
    true
  };

  // Stop tmux server if user confirmed
  if restart_tmux && tmux_was_running {
    tmux::stop_server(&ctx.config, true)?;
  }

  // Restart daemon (stop may fail if not running; ignore and proceed)
  let _ = stop_daemon_only();
  start()?;

  // Ensure tmux server is running (already handled by start(), but explicit for clarity)
  tmux::ensure_server(&ctx.config)?;

  Ok(())
}

/// Show the status of the daemon and tmux server.
#[allow(clippy::unnecessary_wraps)]
pub fn status(ctx: &AppContext) -> Result<()> {
  let socket = compute_socket_path(&ctx.config);
  let tmux_socket = tmux::tmux_socket_path(&ctx.config);

  // Check daemon status
  let daemon_running = UnixStream::connect(&socket).is_ok();
  let daemon_status = if daemon_running { "running" } else { "stopped" };

  // Check tmux status
  let tmux_running = tmux::is_server_running(&ctx.config);
  let tmux_status = if tmux_running { "running" } else { "stopped" };

  // Count tmux sessions if running
  let session_count = if tmux_running {
    count_tmux_sessions(&ctx.config)
  } else {
    0
  };

  println!("Daemon:  {daemon_status}");
  println!("  Socket: {}", socket.display());
  println!();
  println!("Tmux:    {tmux_status}");
  println!("  Socket: {}", tmux_socket.display());
  if tmux_running {
    println!("  Sessions: {session_count}");
  }

  Ok(())
}

fn count_tmux_sessions(cfg: &crate::config::AgencyConfig) -> usize {
  let output = std::process::Command::new("tmux")
    .args(tmux::tmux_args_base(cfg))
    .arg("list-sessions")
    .arg("-F")
    .arg("#{session_name}")
    .output();

  let Ok(out) = output else {
    return 0;
  };

  if !out.status.success() {
    return 0;
  }

  let text = String::from_utf8_lossy(&out.stdout);
  text
    .lines()
    .filter(|name| name.starts_with("agency-"))
    .count()
}
