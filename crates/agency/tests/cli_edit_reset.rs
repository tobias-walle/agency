mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;

#[test]
fn edit_opens_markdown_via_editor() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("edit-task", &["--draft"])?;

    env
      .agency()?
      .arg("edit")
      .arg(id.to_string())
      .assert()
      .success();

    Ok(())
  })
}

#[test]
fn reset_prunes_worktree_and_branch_keeps_markdown() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("reset-task", &["--draft"])?;

    env.bootstrap_task(id)?;
    assert!(env.branch_exists(id, &slug)?);
    assert!(env.worktree_dir_path(id, &slug).is_dir());
    assert!(env.task_file_path(id, &slug).is_file());

    env
      .agency()?
      .arg("reset")
      .arg(id.to_string())
      .assert()
      .success();

    assert!(!env.branch_exists(id, &slug)?);
    assert!(!env.worktree_dir_path(id, &slug).exists());
    assert!(env.task_file_path(id, &slug).is_file());

    env
      .agency()?
      .arg("reset")
      .arg(id.to_string())
      .assert()
      .success();

    Ok(())
  })
}
