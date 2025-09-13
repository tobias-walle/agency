use assert_cmd::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};
use test_support::init_repo_with_initial_commit;

#[test]
fn attach_no_replay_suppresses_history() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // Initialize a git repository and initial commit
  let _repo = init_repo_with_initial_commit(&root);

  // init project
  let mut init = Command::cargo_bin("agency").expect("compile bin");
  init
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["init"])
    .assert()
    .success();

  // new task (auto-starts by default)
  let mut newc = Command::cargo_bin("agency").expect("compile bin");
  newc
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["new", "--agent", "fake", "--no-attach", "feat-no-replay"])
    .assert()
    .success();

  // attach and generate some output, then detach
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["attach", "feat-no-replay"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = attach.spawn().expect("spawn attach");

  // write stdin: echo lines then Ctrl-Q (0x11)
  {
    let stdin = child.stdin.as_mut().expect("stdin");
    let bytes = [b"echo first\n" as &[u8], b"echo second\n" as &[u8], &[0x11u8]].concat();
    stdin.write_all(&bytes).unwrap();
  }

  let out = child.wait_with_output().expect("wait");
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert!(stdout.contains("first") && stdout.contains("second"));

  // re-attach with --no-replay; expect first read to not include history
  let mut attach2 = Command::cargo_bin("agency").expect("compile bin");
  attach2
    .env("AGENCY_SOCKET", sock.as_os_str())
    .current_dir(&root)
    .args(["attach", "--no-replay", "feat-no-replay"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child2 = attach2.spawn().expect("spawn attach2");

  // Don't send any input; just wait a short moment and then detach
  std::thread::sleep(std::time::Duration::from_millis(150));
  {
    let stdin = child2.stdin.as_mut().expect("stdin");
    let bytes = vec![0x11u8]; // Ctrl-Q
    stdin.write_all(&bytes).unwrap();
  }

  let out2 = child2.wait_with_output().expect("wait2");
  assert!(out2.status.success());
  let stdout2 = String::from_utf8_lossy(&out2.stdout);
  // Should not contain the previous history lines
  assert!(
    !stdout2.contains("first") && !stdout2.contains("second"),
    "stdout2: {}",
    stdout2
  );
}
