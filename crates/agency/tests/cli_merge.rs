mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;

#[test]
fn merge_keeps_task_intact() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("merge-keep", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    let old_main = env.git_branch_head_id("main")?;

    env
      .agency()?
      .arg("merge")
      .arg(id.to_string())
      .assert()
      .success();

    let new_main = env.git_branch_head_id("main")?;
    assert_ne!(old_main, new_main, "main should advance after merge");

    // Task artifacts should REMAIN after merge (non-destructive)
    assert!(
      env.branch_exists(id, &slug)?,
      "branch should remain after merge"
    );
    assert!(
      env.task_file_path(id, &slug).exists(),
      "task file should remain after merge"
    );
    assert!(
      env.worktree_dir_path(id, &slug).exists(),
      "worktree should remain after merge"
    );

    Ok(())
  })
}

#[test]
fn merge_logs_complete_hint() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("merge-hint", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    let output = env.agency()?.arg("merge").arg(id.to_string()).output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
      stdout.contains("agency complete"),
      "stdout should contain hint about agency complete command: {stdout}",
    );

    Ok(())
  })
}

#[test]
fn merge_stashes_and_restores_dirty_base() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("merge-dirty", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    let tracked = env.write_file("tracked.txt", "v1")?;
    env.git_add_all_and_commit("add tracked")?;
    std::fs::write(&tracked, b"v2")?;

    env
      .agency()?
      .arg("merge")
      .arg(id.to_string())
      .assert()
      .success();

    let contents = std::fs::read_to_string(&tracked)?;
    assert_eq!(contents, "v2");

    let stash_list = env.git_stash_list()?;
    assert!(
      stash_list.trim().is_empty(),
      "expected no lingering stash entries"
    );

    // Task artifacts should REMAIN after merge (non-destructive)
    assert!(
      env.branch_exists(id, &slug)?,
      "branch should remain after merge"
    );
    assert!(
      env.task_file_path(id, &slug).exists(),
      "task file should remain"
    );
    assert!(
      env.worktree_dir_path(id, &slug).exists(),
      "worktree should remain"
    );

    Ok(())
  })
}

#[test]
fn merge_refreshes_checked_out_base_worktree() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("merge-refresh", &["--draft"])?;
    env.bootstrap_task(id)?;

    let _ = env.git_commit_empty_tree_to_task_branch(id, &slug, "test")?;

    let output = env.agency()?.arg("merge").arg(id.to_string()).output()?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Refreshed checked-out working tree"));

    let status = env.git_status_porcelain()?;
    assert!(status.trim().is_empty());

    // Task artifacts should REMAIN after merge (non-destructive)
    assert!(
      env.branch_exists(id, &slug)?,
      "branch should remain after merge"
    );
    assert!(
      env.task_file_path(id, &slug).exists(),
      "task file should remain"
    );

    Ok(())
  })
}

#[test]
fn merge_fails_when_no_changes() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("merge-no-changes", &["--draft"])?;
    env.bootstrap_task(id)?;

    let output = env.agency()?.arg("merge").arg(id.to_string()).output()?;
    assert!(
      !output.status.success(),
      "merge unexpectedly succeeded for no-op task"
    );

    assert!(
      env.branch_exists(id, &slug)?,
      "branch should remain after failed merge"
    );
    assert!(
      env.task_file_path(id, &slug).exists(),
      "task file should remain"
    );
    assert!(
      env.worktree_dir_path(id, &slug).exists(),
      "worktree should remain"
    );

    Ok(())
  })
}
