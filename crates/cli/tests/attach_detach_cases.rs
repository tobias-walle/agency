use assert_cmd::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};
use test_support::init_repo_with_initial_commit;

#[test]
fn attach_roundtrip_custom_detach_env_case_insensitive() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // Initialize git repo
  let _repo = init_repo_with_initial_commit(&root);

  // init project
  Command::cargo_bin("agency")
    .unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"])
    .assert()
    .success();

  // new task
  Command::cargo_bin("agency")
    .unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "--agent", "fake", "--no-attach", "feat-ci"])
    .assert()
    .success();

  // attach; set custom detach keys via env with mixed case
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .env("AGENCY_DETACH_KEYS", "CTRL-P, Ctrl-Q")
    .current_dir(&root)
    .args(["attach", "feat-ci"]) // uses AGENCY_SOCKET
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

#[test]
fn attach_roundtrip_detach_env_ignores_non_ctrl_letter() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // Initialize git repo
  let _repo = init_repo_with_initial_commit(&root);

  // init project
  Command::cargo_bin("agency")
    .unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"])
    .assert()
    .success();

  // new task
  Command::cargo_bin("agency")
    .unwrap()
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "--agent", "fake", "--no-attach", "feat-ignore"])
    .assert()
    .success();

  // attach; include a non-letter combo alongside ctrl-q
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .env("AGENCY_DETACH_KEYS", "alt-enter, ctrl-q")
    .current_dir(&root)
    .args(["attach", "feat-ignore"]) // uses AGENCY_SOCKET
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = attach.spawn().expect("spawn attach");

  // write stdin: echo hi then Ctrl-Q (0x11)
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
