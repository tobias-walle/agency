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
fn shell_exit_triggers_exited_and_restart() -> Result<()> {
  let td = tempdir()?;
  let workdir = td.path();

  // Ensure fake agent is present for daemon
  ensure_fake_agent(workdir)?;

  let bin = bin();
  let mut daemon = spawn_daemon(&bin, workdir)?;
  wait_for_socket(&workdir.join("tmp/agency.sock"), Duration::from_secs(5))?;

  // Prepare a minimal task file
  let tasks_dir = workdir.join(".agency").join("tasks");
  std::fs::create_dir_all(&tasks_dir)?;
  std::fs::write(tasks_dir.join("1-alpha.md"), "# Task 1: alpha\n")?;

  let mut sess = spawn_attach_pty(&bin, workdir, "alpha")?;
  // Allow more time for restart synchronization in this test
  sess.set_expect_timeout(Some(Duration::from_secs(5)));

  // Cause the shell in the PTY to exit
  sess.send_line("exit")?;
  // Wait for the Exited stats message to confirm restart happened
  sess.expect("===== Session Stats =====")?;

  // After restart, the session should still respond to input
  sess.send_line("echo AFTER")?;
  sess.expect("AFTER")?;

  // Detach via Ctrl-Q and expect client to exit
  send_ctrl_q(&mut sess)?;
  sess.expect(expectrl::Eof)?;

  let _ = daemon.kill();
  let _ = daemon.wait();

  Ok(())
}
