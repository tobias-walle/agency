mod common;

use anyhow::Result;
use predicates::prelude::*;
use crate::common::test_env::TestEnv;

#[test]
fn path_prints_absolute_worktree_path_by_id_and_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("beta-task", &[])?;

    let expected = env.worktree_dir_path(id, &slug);
    let expected_canon = expected.canonicalize().unwrap_or(expected.clone());
    let expected_str = expected_canon.display().to_string() + "\n";

    env
      .agency()?
      .arg("path")
      .arg(id.to_string())
      .assert()
      .success()
      .stdout(predicates::str::contains(expected_str.clone()).from_utf8());

    env
      .agency()?
      .arg("path")
      .arg(&slug)
      .assert()
      .success()
      .stdout(predicates::str::contains(expected_str).from_utf8());

    Ok(())
  })
}

#[test]
fn branch_prints_branch_name_by_id_and_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("gamma-task", &[])?;

    env
      .agency()?
      .arg("branch")
      .arg(id.to_string())
      .assert()
      .success()
      .stdout(predicates::str::contains(env.branch_name(id, &slug)).from_utf8());

    env
      .agency()?
      .arg("branch")
      .arg(&slug)
      .assert()
      .success()
      .stdout(predicates::str::contains(env.branch_name(id, &slug)).from_utf8());

    Ok(())
  })
}
