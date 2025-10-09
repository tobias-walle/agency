use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn requires_agent_when_not_configured_and_no_flag() {
  // Use a directory as AGENCY_SOCKET to prevent a daemon from binding/starting reliably
  let sock_dir = std::env::temp_dir();
  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let output = cmd
    .env("AGENCY_SOCKET", &sock_dir)
    .args(["new", "feat-xyz"]) // no --agent and no config
    .assert()
    .failure()
    .get_output()
    .stderr
    .clone();
  let err = String::from_utf8_lossy(&output);
  assert!(
    err.contains("no agent specified"),
    "expected actionable error, got: {}",
    err
  );
}

#[test]
fn new_with_fake_agent_and_message_writes_body_and_starts_running() {
  // Create unique socket path
  let mut sock = std::env::temp_dir();
  sock.push(format!("agency-test-{}.sock", std::process::id()));
  std::fs::create_dir_all(sock.parent().unwrap()).unwrap();

  // Temp workspace with git repo
  let td = tempfile::tempdir().unwrap();
  let root = td.path();
  // Initialize repo with initial commit on main
  test_support::init_repo_with_initial_commit(root);

  // Start daemon
  let mut start = Command::cargo_bin("agency").expect("compile bin");
  start
    .env("AGENCY_SOCKET", &sock)
    .current_dir(root)
    .args(["daemon", "start"]) // fire-and-wait
    .assert()
    .success();

  // Run new with body and fake agent
  let mut new_cmd = Command::cargo_bin("agency").expect("compile bin");
  let out = new_cmd
    .env("AGENCY_SOCKET", &sock)
    .current_dir(root)
    .args([
      "new",
      "--agent",
      "fake",
      "--no-attach",
      "-m",
      "Body",
      "feat-abc",
    ])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&out);
  assert!(text.contains("feat-abc"), "stdout: {}", text);
  assert!(text.contains("Running"), "stdout: {}", text);

  // Validate the task file exists and contains the body
  let task_path = root.join(".agency").join("tasks").join("1-feat-abc.md");
  let md = std::fs::read_to_string(&task_path).expect("task markdown");
  assert!(md.contains("Body"), "markdown did not contain body: {}", md);

  // Stop daemon to clean up
  let mut stop = Command::cargo_bin("agency").expect("compile bin");
  stop
    .env("AGENCY_SOCKET", &sock)
    .current_dir(root)
    .args(["daemon", "stop"]) // best effort
    .assert()
    .success();
}
