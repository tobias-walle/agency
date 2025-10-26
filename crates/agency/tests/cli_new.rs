mod common;

use std::process::Command;

use anyhow::Result;
use assert_cmd::prelude::*;

#[test]
fn creates_tasks_dir() -> Result<()> {
  let env = common::TestEnv::new();

  let mut cmd = Command::cargo_bin("agency")?;
  cmd.current_dir(env.path());
  cmd.arg("new").arg("märchen-test");

  cmd.assert().success();

  let tasks = env.path().join(".agency").join("tasks");
  assert!(
    tasks.is_dir(),
    "tasks dir should be created at {}",
    tasks.display()
  );

  Ok(())
}
