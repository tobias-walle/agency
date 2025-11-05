#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use std::{fs, thread};

use anyhow::Context;
use expectrl::Expect;
use expectrl::session::OsSession as Session;

pub fn bin() -> PathBuf {
  assert_cmd::cargo::cargo_bin!("agency").to_path_buf()
}

pub fn spawn_daemon(bin: &Path, workdir: &Path) -> anyhow::Result<Child> {
  let mut cmd = Command::new(bin);
  cmd
    .arg("daemon")
    .env("RUST_LOG", "debug")
    .current_dir(workdir);
  let child = cmd.spawn().context("failed to spawn daemon")?;
  Ok(child)
}

pub fn wait_for_socket(sock: &Path, timeout: Duration) -> anyhow::Result<()> {
  let start = Instant::now();
  while start.elapsed() < timeout {
    if fs::metadata(sock).is_ok() {
      return Ok(());
    }
    thread::sleep(Duration::from_millis(50));
  }
  anyhow::bail!("socket not created at {}", sock.display());
}

pub fn spawn_attach_pty(bin: &Path, workdir: &Path) -> anyhow::Result<Session> {
  // Ensure attach runs in the same temp working directory as the daemon
  // so it resolves `./tmp/daemon.sock` correctly.
  let prev = std::env::current_dir()?;
  std::env::set_current_dir(workdir)?;

  let cmd = format!("{} attach", bin.display());
  let mut sess = expectrl::spawn(cmd).context("failed to spawn attach client")?;
  sess.set_expect_timeout(Some(Duration::from_secs(2)));

  // Restore previous working directory for the test process.
  std::env::set_current_dir(prev)?;
  Ok(sess)
}

pub fn send_ctrl_c(sess: &mut Session) -> anyhow::Result<()> {
  sess.send("\x03")?;
  Ok(())
}
