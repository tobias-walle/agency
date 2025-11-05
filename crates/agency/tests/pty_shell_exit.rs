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

  let bin = bin();
  let mut daemon = spawn_daemon(&bin, workdir)?;
  wait_for_socket(&workdir.join("tmp/daemon.sock"), Duration::from_secs(5))?;

  let mut sess = spawn_attach_pty(&bin, workdir)?;
  // Allow more time for restart synchronization in this test
  sess.set_expect_timeout(Some(Duration::from_secs(5)));

  // Cause the shell in the PTY to exit
  sess.send_line("exit")?;
  // Wait for the Exited stats message to confirm restart happened
  sess.expect("===== Session Stats =====")?;

  // After restart, the session should still respond to input
  sess.send_line("echo AFTER")?;
  sess.expect("AFTER")?;

  // Detach via Ctrl-C and expect client to exit
  send_ctrl_c(&mut sess)?;
  sess.expect(expectrl::Eof)?;

  let _ = daemon.kill();
  let _ = daemon.wait();

  Ok(())
}
