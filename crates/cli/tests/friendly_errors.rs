use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn new_task_shows_actionable_daemon_not_reachable_message() {
  // Point to a non-existent socket path
  let mut sock = std::env::temp_dir();
  sock.push(format!("agency-missing-{}.sock", std::process::id()));
  // Ensure it doesn't exist
  let _ = std::fs::remove_file(&sock);

  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let output = cmd
    .env("AGENCY_SOCKET", &sock)
    .args(["new", "test-slug"])
    .assert()
    .failure()
    .get_output()
    .stderr
    .clone();
  let err = String::from_utf8_lossy(&output);
  assert!(err.contains("daemon not reachable"), "stderr: {}", err);
  assert!(err.contains("agency daemon start"), "stderr: {}", err);
  assert!(err.contains(&*sock.to_string_lossy()), "stderr: {}", err);
}
