use assert_cmd::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};
use test_support::init_repo_with_initial_commit;

#[test]
fn attach_help_has_no_detach_flag() {
  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
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
  let sock = td.path().join("agency.sock");

  // Initialize a git repository in the temp directory and make an initial commit
  let _repo = init_repo_with_initial_commit(&root);

  // init project (does not require daemon)
  let mut init = Command::cargo_bin("agency").expect("compile bin");
  init
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"])
    .assert()
    .success();

  // new task (autostarts daemon; starts running by default)
  let mut newc = Command::cargo_bin("agency").expect("compile bin");
  newc
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "feat-e2e"])
    .assert()
    .success();

  // attach interactively; send echo and then Ctrl-Q (0x11)
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["attach", "feat-e2e"]) // uses AGENCY_SOCKET
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
}

#[test]
fn attach_roundtrip_custom_detach_env_ctrl_p_ctrl_q() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // Initialize git repo
  let _repo = init_repo_with_initial_commit(&root);

  // init project
  let mut init = Command::cargo_bin("agency").expect("compile bin");
  init
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"])
    .assert()
    .success();

  // new task (autostarts and runs by default)
  let mut newc = Command::cargo_bin("agency").expect("compile bin");
  newc
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "feat-e2e"])
    .assert()
    .success();

  // attach; set custom detach keys via env
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .env("AGENCY_DETACH_KEYS", "ctrl-p,ctrl-q")
    .current_dir(&root)
    .args(["attach", "feat-e2e"]) // uses AGENCY_SOCKET
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = attach.spawn().expect("spawn attach");

  // write stdin: echo hi then Ctrl-P (0x10) + Ctrl-Q (0x11)
  {
    let stdin = child.stdin.as_mut().expect("stdin");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"echo hi\n");
    bytes.push(0x10); // Ctrl-P
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
}
