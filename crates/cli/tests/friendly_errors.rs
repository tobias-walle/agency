use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn new_task_shows_actionable_daemon_not_reachable_message() {
  // Point to a directory path as socket -> daemon cannot bind
  let sock_dir = std::env::temp_dir();

  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let output = cmd
    .env("AGENCY_SOCKET", &sock_dir)
    .args(["new", "--agent", "fake", "--no-attach", "test-slug"])
    .assert()
    .failure()
    .get_output()
    .stderr
    .clone();
  let err = String::from_utf8_lossy(&output);
  assert!(err.contains("daemon not reachable"), "stderr: {}", err);
  assert!(
    err.contains(&*sock_dir.to_string_lossy()),
    "stderr: {}",
    err
  );
}
