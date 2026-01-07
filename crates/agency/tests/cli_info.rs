mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn info_displays_task_details() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("test-info", &[])?;
    env.bootstrap_task(id)?;

    let wt_dir = env.worktree_dir_path(id, &slug);

    env
      .agency()?
      .current_dir(&wt_dir)
      .env("AGENCY_TASK_ID", id.to_string())
      .arg("info")
      .assert()
      .success()
      .stdout(
        predicates::str::contains(format!("Task: {}-{}", id, slug))
          .from_utf8()
      )
      .stdout(predicates::str::contains("Base:").from_utf8())
      .stdout(predicates::str::contains("Agent:").from_utf8())
      .stdout(predicates::str::contains("Worktree:").from_utf8())
      .stdout(predicates::str::contains("Files:").from_utf8());

    Ok(())
  })
}

#[test]
fn info_shows_no_files_when_empty() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("no-files", &[])?;
    env.bootstrap_task(id)?;

    let wt_dir = env.worktree_dir_path(id, &slug);

    env
      .agency()?
      .current_dir(&wt_dir)
      .env("AGENCY_TASK_ID", id.to_string())
      .arg("info")
      .assert()
      .success()
      .stdout(predicates::str::contains("No files attached.").from_utf8());

    Ok(())
  })
}

#[test]
fn info_fails_outside_worktree() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("info")
      .assert()
      .failure();

    Ok(())
  })
}

#[test]
fn info_displays_base_branch_from_task_file() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    // Create a feature branch
    env.git_create_branch("feature-branch")?;
    env.git_checkout("feature-branch")?;

    // Create task on the feature branch
    let (id, slug) = env.new_task("feature-task", &[])?;
    env.bootstrap_task(id)?;

    let wt_dir = env.worktree_dir_path(id, &slug);

    // Info should show feature-branch as base
    env
      .agency()?
      .current_dir(&wt_dir)
      .env("AGENCY_TASK_ID", id.to_string())
      .arg("info")
      .assert()
      .success()
      .stdout(predicates::str::contains("Base: feature-branch").from_utf8());

    Ok(())
  })
}
