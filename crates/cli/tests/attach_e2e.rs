use assert_cmd::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};

fn start_daemon(sock: &std::path::Path) {
  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  let out = cmd
    .env("ORCHESTRA_SOCKET", sock.as_os_str())
    .args(["daemon", "start"]) // starts background
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&out);
  assert!(text.contains("daemon: running"), "{text}");
}

fn stop_daemon(sock: &std::path::Path) {
  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  let _ = cmd
    .env("ORCHESTRA_SOCKET", sock.as_os_str())
    .args(["daemon", "stop"]) // best-effort
    .assert()
    .success();
}

#[test]
fn attach_help_has_no_detach_flag() {
  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  let out = cmd
    .args(["attach", "--help"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&out);
  assert!(
    !text.to_lowercase().contains("detach-keys"),
    "help shows detach flag unexpectedly: {}",
    text
  );
}

#[test]
fn attach_roundtrip_default_detach_ctrl_q() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("orchestra.sock");

  // Initialize a git repository in the temp directory and make an initial commit
  std::process::Command::new("git")
    .arg("init")
    .current_dir(&root)
    .output()
    .expect("failed to init git repo");
  std::fs::write(root.join("README.md"), "# E2E Test\n").unwrap();
  std::process::Command::new("git")
    .args(["add", "."])
    .current_dir(&root)
    .output()
    .expect("failed to git add");
  std::process::Command::new("git")
    .args(["commit", "-m", "Initial commit"])
    .current_dir(&root)
    .output()
    .expect("failed to git commit");

  start_daemon(&sock);

  // init project
  let mut init = Command::cargo_bin("orchestra").expect("compile bin");
  init
    .env("ORCHESTRA_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"])
    .assert()
    .success();

  // new task
  let mut newc = Command::cargo_bin("orchestra").expect("compile bin");
  newc
    .env("ORCHESTRA_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "feat-e2e", "--title", "E2E Task"])
    .assert()
    .success();

  // start task
  let mut start = Command::cargo_bin("orchestra").expect("compile bin");
  start
    .env("ORCHESTRA_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["start", "feat-e2e"])
    .assert()
    .success();

  // attach interactively; send echo and then Ctrl-Q (0x11)
  let mut attach = Command::cargo_bin("orchestra").expect("compile bin");
  attach
    .env("ORCHESTRA_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["attach", "feat-e2e"]) // uses ORCHESTRA_SOCKET
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = attach.spawn().expect("spawn attach");

  // write stdin: echo hi then detach
  {
    let stdin = child.stdin.as_mut().expect("stdin");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"echo hi\n");
    bytes.push(0x11); // Ctrl-Q
    stdin.write_all(&bytes).unwrap();
  }

  let out = child.wait_with_output().expect("wait");
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout);
  let stderr = String::from_utf8_lossy(&out.stderr);
  assert!(stdout.contains("Attached. Detach:"), "stdout: {}", stdout);
  assert!(stdout.contains("hi"), "stdout: {}", stdout);
  assert!(stderr.contains("detached"), "stderr: {}", stderr);

  stop_daemon(&sock);
}
