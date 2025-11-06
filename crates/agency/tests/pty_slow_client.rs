#![cfg(unix)]

use std::time::Duration;

use anyhow::Result;
use expectrl::Expect;
use serial_test::serial;
use tempfile::tempdir;

mod pty_helpers;
use pty_helpers::*;

#[test]
#[serial]
fn control_priority_under_heavy_output() -> Result<()> {
  let td = tempdir()?;
  let workdir = td.path();

  let bin = bin();
  let mut daemon = spawn_daemon(&bin, workdir)?;
  wait_for_socket(&workdir.join("tmp/agency.sock"), Duration::from_secs(5))?;

  // Prepare a minimal task file
  let tasks_dir = workdir.join(".agency").join("tasks");
  std::fs::create_dir_all(&tasks_dir)?;
  std::fs::write(tasks_dir.join("1-alpha.md"), "# Task 1: alpha\n")?;

  let mut sess = spawn_attach_pty(&bin, workdir, "alpha")?;

  // Generate heavy output in the PTY (about 1MB)
  sess.send_line("yes X | head -c 1000000")?;

  // Quickly request detach while output is streaming
  send_ctrl_q(&mut sess)?;

  // Expect the client to exit promptly (Goodbye processed despite heavy output)
  sess.expect(expectrl::Eof)?;

  let _ = daemon.kill();
  let _ = daemon.wait();

  Ok(())
}
