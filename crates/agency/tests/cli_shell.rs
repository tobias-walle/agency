mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn shell_fails_when_worktree_not_found() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, _slug) = env.new_task("no-worktree", &[])?;

    env
      .agency()?
      .arg("shell")
      .arg(id.to_string())
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("worktree not found")
          .from_utf8()
      )
      .stderr(
        predicates::str::contains("Run `agency bootstrap")
          .from_utf8()
      );

    Ok(())
  })
}

#[test]
fn shell_fails_with_invalid_task_id() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("shell")
      .arg("999")
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("not found")
          .or(predicates::str::contains("No such"))
          .from_utf8()
      );

    Ok(())
  })
}

#[test]
fn shell_accepts_task_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (_id, slug) = env.new_task("shell-test", &[])?;

    // Should fail with worktree not found (since not bootstrapped)
    // but should accept the slug
    env
      .agency()?
      .arg("shell")
      .arg(&slug)
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("worktree not found")
          .from_utf8()
      );

    Ok(())
  })
}
