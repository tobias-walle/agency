use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn status_autostarts_daemon() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // init project layout (does not need daemon)
  let mut init = Command::cargo_bin("agency").expect("compile bin");
  init
    .current_dir(&root)
    .args(["init"]) // no socket needed
    .assert()
    .success();

  // status should autostart daemon and succeed
  let mut status = Command::cargo_bin("agency").expect("compile bin");
  let out = status
    .env("AGENCY_SOCKET", &sock)
    .current_dir(&root)
    .args(["status"]) // task listing
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let text = String::from_utf8_lossy(&out);
  assert!(text.contains("ID   SLUG                 STATUS"), "{text}");
}
