use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn init_creates_orchestra_layout_and_config() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path();

  let mut cmd = Command::cargo_bin("orchestra").expect("compile bin");
  let _out = cmd
    .current_dir(root)
    .args(["init"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();

  assert!(root.join(".orchestra").exists());
  assert!(root.join(".orchestra/tasks").exists());
  assert!(root.join(".orchestra/worktrees").exists());
  assert!(root.join(".orchestra/config.toml").exists());
}
