#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use assert_cmd::prelude::*;
use serial_test::serial;

mod pty_helpers;
use pty_helpers::*;
mod common;

#[test]
#[serial]
fn daemon_start_stop_restart_creates_and_removes_socket() -> Result<()> {
  // Skip on sandboxes that disallow PTY allocation
  if !pty_available() {
    eprintln!("Skipping daemon test: PTY not available in sandbox");
    return Ok(());
  }
  let td = common::tempdir_in_sandbox();
  let workdir = td.path();
  let runtime = runtime_dir_for(workdir);
  let sock = runtime.join("agency.sock");

  // Ensure fake agent is present for daemon
  ensure_fake_agent(workdir)?;

  // start
  let mut cmd = new_cmd_in_runtime(workdir, &runtime);
  cmd.arg("daemon").arg("start");
  cmd.assert().success();

  wait_for_socket(&sock, Duration::from_secs(5))?;

  // stop
  let mut cmd = new_cmd_in_runtime(workdir, &runtime);
  cmd.arg("daemon").arg("stop");
  cmd.assert().success();

  // poll disappearance
  let start = std::time::Instant::now();
  while start.elapsed() < Duration::from_secs(5) {
    if std::fs::metadata(&sock).is_err() {
      break;
    }
    std::thread::sleep(Duration::from_millis(50));
  }

  // restart
  let mut cmd = new_cmd_in_runtime(workdir, &runtime);
  cmd.arg("daemon").arg("restart");
  cmd.assert().success();

  wait_for_socket(&sock, Duration::from_secs(5))?;

  Ok(())
}

fn bin() -> PathBuf {
  assert_cmd::cargo::cargo_bin!("agency").to_path_buf()
}

fn pty_available() -> bool {
  use portable_pty::{PtySize, native_pty_system};
  let pty = native_pty_system();
  pty
    .openpty(PtySize {
      rows: 1,
      cols: 1,
      pixel_width: 0,
      pixel_height: 0,
    })
    .map(drop)
    .is_ok()
}
