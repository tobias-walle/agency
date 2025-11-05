#![cfg(unix)]

use std::time::Duration;

use anyhow::Result;
use expectrl::Eof;
use expectrl::Expect;
use serial_test::serial;
use tempfile::tempdir;

mod pty_helpers;
use pty_helpers::*;

#[test]
#[serial]
fn roundtrip_attach_and_detach() -> Result<()> {
  let td = tempdir()?;
  let workdir = td.path();

  let bin = bin();
  let mut daemon = spawn_daemon(&bin, workdir)?;

  wait_for_socket(&workdir.join("tmp/daemon.sock"), Duration::from_secs(5))?;

  let mut sess = spawn_attach_pty(&bin, workdir)?;

  sess.send_line("echo READY")?;
  sess.expect("READY")?;

  // Detach via Ctrl-C and expect client to exit
  send_ctrl_c(&mut sess)?;
  sess.expect(Eof)?;

  // Shutdown daemon
  let _ = daemon.kill();
  let _ = daemon.wait();

  Ok(())
}

#[test]
#[serial]
fn reject_second_attach_while_busy() -> Result<()> {
  let td = tempdir()?;
  let workdir = td.path();

  let bin = bin();
  let mut daemon = spawn_daemon(&bin, workdir)?;
  wait_for_socket(&workdir.join("tmp/daemon.sock"), Duration::from_secs(5))?;

  // First attach succeeds
  let mut sess1 = spawn_attach_pty(&bin, workdir)?;
  sess1.send_line("echo READY1")?;
  sess1.expect("READY1")?;

  // Second attach should be rejected
  let mut sess2 = spawn_attach_pty(&bin, workdir)?;
  sess2.send_line("echo READY2")?;
  sess2.expect("Another client is attached")?;

  // Detach the first client
  send_ctrl_c(&mut sess1)?;
  sess1.expect(Eof)?;

  // Third attach should now work
  let mut sess3 = spawn_attach_pty(&bin, workdir)?;
  sess3.send_line("echo OK")?;
  sess3.expect("OK")?;
  send_ctrl_c(&mut sess3)?;
  sess3.expect(Eof)?;

  let _ = daemon.kill();
  let _ = daemon.wait();

  Ok(())
}
