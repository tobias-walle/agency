mod common;

use anyhow::Result;
use predicates::prelude::*;

use crate::common::test_env::TestEnv;

#[test]
fn fzf_missing_shows_error() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    env.new_task("test-task", &[])?;

    // Run with empty PATH so fzf is not found
    env.with_env_vars(&[("PATH", Some(String::new()))], |env| {
      env
        .agency()
        .expect("build agency cmd")
        .arg("fzf")
        .assert()
        .failure()
        .stderr(predicate::str::contains("fzf is not installed").from_utf8());
    });

    Ok(())
  })
}

#[test]
fn fzf_no_tasks_shows_error() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("fzf")
      .assert()
      .failure()
      .stderr(predicate::str::contains("No tasks found").from_utf8());

    Ok(())
  })
}
