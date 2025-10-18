use assert_cmd::prelude::*;
use std::io::Write;
use std::process::{Command, Stdio};
use test_support::init_repo_with_initial_commit;

#[test]
fn attach_emits_reset_footer_on_detach() {
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
    .args(["new", "--agent", "fake", "--no-attach", "feat-reset-footer"])
    .assert()
    .success();

  // attach and then detach; force reset footer emission even when stdout is piped
  let mut attach = Command::cargo_bin("agency").expect("compile bin");
  attach
    .env("AGENCY_SOCKET", sock.as_os_str())
    .env("AGENCY_FORCE_TTY_RESET", "1")
    .current_dir(&root)
    .args(["attach", "feat-reset-footer"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = attach.spawn().expect("spawn attach");

  // write stdin: echo hi then Ctrl-Q (0x11)
  {
    let stdin = child.stdin.as_mut().expect("stdin");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"echo hi\\n");
    // also emit an OSC sequence to mutate cursor color as a placeholder (not required)
    bytes.extend_from_slice(b"printf '\x1b]12;red\x07'\\n");
    bytes.push(0x11); // Ctrl-Q
    stdin.write_all(&bytes).unwrap();
  }

  let out = child.wait_with_output().expect("wait");
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout);
  // Assert essential sequences appear in stdout footer
  assert!(
    stdout.contains("\x1b]112\x1b\\"),
    "missing OSC 112 reset: {}",
    stdout
  );
  assert!(!stdout.contains("\x1b[!p"), "unexpected DECSTR: {}", stdout);
  assert!(
    stdout.contains("\x1b[?1049l"),
    "missing leave alt-screen: {}",
    stdout
  );
}
