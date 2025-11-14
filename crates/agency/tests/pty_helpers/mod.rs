#![cfg(unix)]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use std::{fs, thread};

use super::common;
use anyhow::Context;
use expectrl::Expect;
use expectrl::session::OsSession as Session;

#[must_use]
pub fn bin() -> PathBuf {
  assert_cmd::cargo::cargo_bin!("agency").to_path_buf()
}

pub fn spawn_daemon(bin: &Path, workdir: &Path) -> anyhow::Result<Child> {
  let mut cmd = Command::new(bin);
  cmd
    .arg("daemon")
    .arg("run")
    .env("RUST_LOG", "debug")
    .env("XDG_RUNTIME_DIR", runtime_dir_for(workdir))
    .current_dir(workdir);
  let child = cmd.spawn().context("failed to spawn daemon")?;
  Ok(child)
}

/// Create a Command for running the agency binary in `workdir` with XDG runtime
/// pointing to `<workdir>/tmp`. Use this for CLI-driven daemon start/stop tests.
pub fn new_cmd_in_runtime(workdir: &Path, runtime_dir: &Path) -> std::process::Command {
  let mut cmd = std::process::Command::new(bin());
  cmd.current_dir(workdir);
  cmd.env("XDG_RUNTIME_DIR", runtime_dir);
  cmd
}

/// Returns a unique runtime dir under `<workdir>/tmp/run-<nanos>` and ensures it exists.
pub fn runtime_dir_for(_workdir: &Path) -> PathBuf {
  common::runtime_dir_create()
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

pub fn spawn_attach_pty(bin: &Path, workdir: &Path, task_ident: &str) -> anyhow::Result<Session> {
  // Ensure attach runs in the same temp working directory as the daemon
  // so it resolves `./tmp/daemon.sock` correctly.
  let prev = std::env::current_dir()?;
  std::env::set_current_dir(workdir)?;

  let cmd = format!("{} attach {}", bin.display(), task_ident);
  let mut sess = expectrl::spawn(cmd).context("failed to spawn attach client")?;
  sess.set_expect_timeout(Some(Duration::from_secs(2)));

  // Restore previous working directory for the test process.
  std::env::set_current_dir(prev)?;
  Ok(sess)
}

pub fn send_ctrl_q(sess: &mut Session) -> anyhow::Result<()> {
  // Ctrl-Q is 0x11 (DC1)
  sess.send("\x11")?;
  Ok(())
}
