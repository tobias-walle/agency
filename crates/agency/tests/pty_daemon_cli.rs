#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use assert_cmd::prelude::*;
use serial_test::serial;
use tempfile::tempdir;

mod pty_helpers;
use pty_helpers::*;

#[test]
#[serial]
fn daemon_start_stop_restart_creates_and_removes_socket() -> Result<()> {
  let td = tempdir()?;
  let workdir = td.path();
  let sock = workdir.join("tmp").join("agency.sock");

  // Ensure fake agent is present for daemon
  ensure_fake_agent(workdir)?;

  // start
  let mut cmd = std::process::Command::new(bin());
  cmd.arg("daemon").arg("start");
  cmd.current_dir(workdir);
  cmd.env("XDG_RUNTIME_DIR", workdir.join("tmp"));
  cmd.assert().success();

  wait_for_socket(&sock, Duration::from_secs(5))?;

  // stop
  let mut cmd = std::process::Command::new(bin());
  cmd.arg("daemon").arg("stop");
  cmd.current_dir(workdir);
  cmd.env("XDG_RUNTIME_DIR", workdir.join("tmp"));
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
  let mut cmd = std::process::Command::new(bin());
  cmd.arg("daemon").arg("restart");
  cmd.current_dir(workdir);
  cmd.env("XDG_RUNTIME_DIR", workdir.join("tmp"));
  cmd.assert().success();

  wait_for_socket(&sock, Duration::from_secs(5))?;

  Ok(())
}

fn bin() -> PathBuf {
  assert_cmd::cargo::cargo_bin!("agency").to_path_buf()
}
