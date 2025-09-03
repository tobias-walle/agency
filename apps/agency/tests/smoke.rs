use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn help_exits_successfully() {
  let mut cmd = Command::cargo_bin("agency").expect("compile bin");
  let assert = cmd.arg("--help").assert();
  assert.success();
}
