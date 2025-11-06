#![cfg(unix)]

use std::time::Duration;

use anyhow::Result;
use assert_cmd::assert::OutputAssertExt;
use expectrl::Expect;
use serial_test::serial;
use tempfile::tempdir;

mod pty_helpers;
use pty_helpers::*;

#[test]
#[serial]
fn attach_and_stop_by_task() -> Result<()> {
  let td = tempdir()?;
  let workdir = td.path();

  // Ensure fake agent is present for daemon
  ensure_fake_agent(workdir)?;

  // Start daemon
  let mut start = std::process::Command::new(bin());
  start.arg("daemon").arg("start");
  start.current_dir(workdir);
  start.env("XDG_RUNTIME_DIR", workdir.join("tmp"));
  start.assert().success();
  wait_for_socket(&workdir.join("tmp/agency.sock"), Duration::from_secs(5))?;

  // Minimal task file
  let tasks_dir = workdir.join(".agency").join("tasks");
  std::fs::create_dir_all(&tasks_dir)?;
  std::fs::write(tasks_dir.join("1-alpha.md"), "# Task 1: alpha\n")?;

  // Attach to task
  let mut sess = spawn_attach_pty(&bin(), workdir, "alpha")?;
  sess.send_line("echo READY")?;
  sess.expect("READY")?;

  // Stop by task (for now global)
  let mut stop = std::process::Command::new(bin());
  stop.arg("stop").arg("alpha");
  stop.current_dir(workdir);
  stop.env("XDG_RUNTIME_DIR", workdir.join("tmp"));
  stop.assert().success();

  // Expect client to eventually EOF
  sess.set_expect_timeout(Some(Duration::from_secs(5)));
  sess.expect(expectrl::Eof)?;

  Ok(())
}

fn bin() -> std::path::PathBuf {
  assert_cmd::cargo::cargo_bin!("agency").to_path_buf()
}
