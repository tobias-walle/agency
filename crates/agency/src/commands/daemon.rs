use std::os::unix::net::UnixStream;
use std::process::Command as ProcCommand;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use log::{info, warn};

use crate::config::{compute_socket_path, load_config};
use crate::pty::daemon as pty_daemon;
use crate::pty::protocol::{C2D, C2DControl, write_frame};
use crate::utils::daemon::connect_daemon_socket;

pub fn run_blocking() -> Result<()> {
  // Initialize env_logger similar to pty-demo main
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .format_timestamp_secs()
    .try_init();

  // Compute socket path from config (project + XDG)
  let cwd = std::env::current_dir()?;
  let cfg = load_config(&cwd)?;
  let socket = compute_socket_path(&cfg);

  pty_daemon::run_daemon(&socket)
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
  let start = Instant::now();
  while start.elapsed() < Duration::from_secs(5) {
    if std::fs::metadata(&socket).is_ok() {
      info!("Started daemon at {}", socket.display());
      return Ok(());
    }
    thread::sleep(Duration::from_millis(50));
  }
  anyhow::bail!(
    "Daemon did not create socket at {} within timeout",
    socket.display()
  )
}

pub fn stop() -> Result<()> {
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

pub fn restart() -> Result<()> {
  // Stop may fail if not running; ignore and proceed
  let _ = stop();
  start()
}
