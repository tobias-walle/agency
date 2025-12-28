mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;

#[test]
fn complete_merges_and_cleans_up() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("complete-cleanup", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    let old_main = env.git_branch_head_id("main")?;

    env
      .agency()?
      .arg("complete")
      .arg(id.to_string())
      .arg("--yes")
      .assert()
      .success();

    let new_main = env.git_branch_head_id("main")?;
    assert_ne!(old_main, new_main, "main should advance after complete");

    // Task artifacts should be REMOVED after complete
    assert!(
      !env.branch_exists(id, &slug)?,
      "branch should be removed after complete"
    );
    assert!(
      !env.task_file_path(id, &slug).exists(),
      "task file should be removed after complete"
    );
    assert!(
      !env.worktree_dir_path(id, &slug).exists(),
      "worktree should be removed after complete"
    );

    Ok(())
  })
}

#[test]
fn complete_with_yes_skips_confirmation() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("complete-yes", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    // Without --yes in non-interactive mode, should still complete
    // because confirm() defaults to true when not interactive
    let output = env
      .agency()?
      .arg("complete")
      .arg(id.to_string())
      .arg("--yes")
      .output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
      stdout.contains("merged and cleaned up"),
      "stdout should confirm task was merged and cleaned up: {stdout}",
    );

    Ok(())
  })
}

#[test]
fn complete_uses_env_var_for_task_id() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("complete-env", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    env
      .agency()?
      .arg("complete")
      .arg("--yes")
      .env("AGENCY_TASK_ID", id.to_string())
      .assert()
      .success();

    // Task artifacts should be REMOVED after complete
    assert!(
      !env.branch_exists(id, &slug)?,
      "branch should be removed after complete"
    );
    assert!(
      !env.task_file_path(id, &slug).exists(),
      "task file should be removed after complete"
    );

    Ok(())
  })
}

#[test]
fn complete_works_when_already_merged() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("complete-already-merged", &["--draft"])?;
    env.bootstrap_task(id)?;

    // No changes made - task branch is same as base
    let output = env
      .agency()?
      .arg("complete")
      .arg(id.to_string())
      .arg("--yes")
      .output()?;
    assert!(
      output.status.success(),
      "complete should succeed even when already merged"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
      stdout.contains("cleaned up"),
      "stdout should indicate cleanup: {stdout}"
    );

    // Task should be cleaned up
    assert!(
      !env.branch_exists(id, &slug)?,
      "branch should be removed after complete"
    );
    assert!(
      !env.task_file_path(id, &slug).exists(),
      "task file should be removed"
    );

    Ok(())
  })
}
