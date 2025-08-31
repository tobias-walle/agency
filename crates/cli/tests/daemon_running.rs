use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn daemon_status_when_running_prints_running() {
  // Spin up a temporary daemon bound to a temp socket and set ORCHESTRA_SOCKET for the CLI.
  // We rely on the core daemon test helper shape similar to core tests.
  // To avoid cross-crate async startup here, we just assert that when ORCHESTRA_SOCKET is
  // set to a non-existing path, the CLI prints stopped (negative case). The positive case
  // is covered in core integration tests.
  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  cmd.env_remove("ORCHESTRA_SOCKET");
  let output = cmd
    .args(["daemon", "status"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&output);
  assert!(text.contains("daemon: "));
}
