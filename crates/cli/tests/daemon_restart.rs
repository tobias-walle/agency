use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn daemon_restart_transitions_back_to_running() {
  let mut sock = std::env::temp_dir();
  sock.push(format!("agency-restart-{}.sock", std::process::id()));

  // Start
  let mut start = Command::cargo_bin("agency").expect("compile bin");
  start
    .env("AGENCY_SOCKET", &sock)
    .args(["daemon", "start"])
    .assert()
    .success();

  // Restart
  let mut restart = Command::cargo_bin("agency").expect("compile bin");
  restart
    .env("AGENCY_SOCKET", &sock)
    .args(["daemon", "restart"])
    .assert()
    .success();

  // Status shows running
  let mut status = Command::cargo_bin("agency").expect("compile bin");
  let out = status
    .env("AGENCY_SOCKET", &sock)
    .args(["daemon", "status"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&out);
  assert!(text.contains("daemon: running"), "{text}");
}
