mod common;

use anyhow::Result;
use predicates::prelude::*;
use crate::common::test_env::TestEnv;

#[test]
fn rm_confirms_and_removes_on_y_or_y() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("delta-task", &[])?;

    env.bootstrap_task(id)?;

    env
      .agency()?
      .arg("rm")
      .arg(id.to_string())
      .write_stdin("n\n")
      .assert()
      .success()
      .stdout(predicates::str::contains("Cancelled").from_utf8());

    assert!(env.branch_exists(id, &slug)?);
    assert!(env.task_file_path(id, &slug).is_file());
    assert!(env.worktree_dir_path(id, &slug).is_dir());

    env
      .agency()?
      .arg("rm")
      .arg(&slug)
      .write_stdin("Y\n")
      .assert()
      .success()
      .stdout(predicates::str::contains("Removed task, branch, and worktree").from_utf8());

    assert!(!env.branch_exists(id, &slug)?);
    assert!(!env.task_file_path(id, &slug).exists());
    assert!(!env.worktree_dir_path(id, &slug).exists());

    Ok(())
  })
}
