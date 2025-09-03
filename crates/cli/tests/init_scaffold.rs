use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn init_creates_agency_layout_and_config() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path();

  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let _out = cmd
    .current_dir(root)
    .args(["init"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();

  assert!(root.join(".agency").exists());
  assert!(root.join(".agency/tasks").exists());
  assert!(root.join(".agency/worktrees").exists());
  assert!(root.join(".agency/config.toml").exists());
}
