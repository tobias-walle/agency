mod common;

use std::time::Duration;

use anyhow::Result;
use assert_cmd::prelude::*;
use expectrl::{Eof, Expect, Session};
use predicates::prelude::*;

#[test]
fn new_creates_markdown_branch_and_worktree() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Run new
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("alpha-task");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("Task alpha-task with id 1 created").from_utf8());

  // Check markdown
  let file = env
    .path()
    .join(".agency")
    .join("tasks")
    .join("1-alpha-task.md");
  assert!(
    file.is_file(),
    "task file should exist at {}",
    file.display()
  );

  // Check branch exists via git2
  let repo = git2::Repository::discover(env.path())?;
  let _branch = repo.find_branch("agency/1-alpha-task", git2::BranchType::Local)?;

  // Check worktree dir exists
  let wt_dir = env
    .path()
    .join(".agency")
    .join("worktrees")
    .join("1-alpha-task");
  assert!(
    wt_dir.is_dir(),
    "worktree dir should exist at {}",
    wt_dir.display()
  );

  Ok(())
}

#[test]
fn new_writes_yaml_header_when_agent_specified() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Run new with agent
  let mut cmd = env.bin_cmd()?;
  cmd
    .arg("new")
    .arg("--no-edit")
    .arg("-a")
    .arg("fake")
    .arg("alpha-task");
  cmd.assert().success();

  // Check markdown content includes YAML front matter
  let file = env
    .path()
    .join(".agency")
    .join("tasks")
    .join("1-alpha-task.md");
  let data = std::fs::read_to_string(&file)?;
  assert!(
    data.starts_with("---\n"),
    "file should start with YAML '---' block"
  );
  assert!(
    data.contains("agent: fake\n"),
    "front matter should contain agent: fake"
  );
  assert!(
    data.contains("\n---\n\n# Task 1: alpha-task\n"),
    "should close YAML and include title"
  );

  Ok(())
}

#[test]
fn path_prints_absolute_worktree_path_by_id_and_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Create
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("beta-task");
  cmd.assert().success();

  let expected = env
    .path()
    .join(".agency")
    .join("worktrees")
    .join("1-beta-task");
  let expected_canon = expected.canonicalize().unwrap_or(expected.clone());
  let expected_str = expected_canon.display().to_string() + "\n";

  // path by id
  let mut cmd = env.bin_cmd()?;
  cmd.arg("path").arg("1");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(expected_str.clone()).from_utf8());

  // path by slug
  let mut cmd = env.bin_cmd()?;
  cmd.arg("path").arg("beta-task");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(expected_str).from_utf8());

  Ok(())
}

#[test]
fn branch_prints_branch_name_by_id_and_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Create
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("gamma-task");
  cmd.assert().success();

  // by id
  let mut cmd = env.bin_cmd()?;
  cmd.arg("branch").arg("1");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::diff("agency/1-gamma-task\n").from_utf8());

  // by slug
  let mut cmd = env.bin_cmd()?;
  cmd.arg("branch").arg("gamma-task");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::diff("agency/1-gamma-task\n").from_utf8());

  Ok(())
}

#[test]
fn rm_confirms_and_removes_on_y_or_y() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Create
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("delta-task");
  cmd.assert().success();

  // Run rm and cancel
  let mut cmd = env.bin_cmd()?;
  cmd.arg("rm").arg("1");
  let mut session = Session::spawn(cmd)?;
  session.set_expect_timeout(Some(Duration::from_secs(2)));
  session.send_line("n")?;
  session.expect("Cancelled")?;
  session.expect(Eof)?;

  // Ensure still present
  let repo = git2::Repository::discover(env.path())?;
  assert!(
    repo
      .find_branch("agency/1-delta-task", git2::BranchType::Local)
      .is_ok()
  );
  assert!(
    env
      .path()
      .join(".agency")
      .join("tasks")
      .join("1-delta-task.md")
      .is_file()
  );
  assert!(
    env
      .path()
      .join(".agency")
      .join("worktrees")
      .join("1-delta-task")
      .is_dir()
  );

  // Run rm and confirm with Y
  let mut cmd = env.bin_cmd()?;
  cmd.arg("rm").arg("delta-task");
  let mut session = Session::spawn(cmd)?;
  session.set_expect_timeout(Some(Duration::from_secs(2)));
  session.send_line("Y")?;
  session.expect("Removed task, branch, and worktree")?;
  session.expect(Eof)?;

  // Verify removal
  let repo = git2::Repository::discover(env.path())?;
  assert!(
    repo
      .find_branch("agency/1-delta-task", git2::BranchType::Local)
      .is_err()
  );
  assert!(
    !env
      .path()
      .join(".agency")
      .join("tasks")
      .join("1-delta-task.md")
      .exists()
  );
  assert!(
    !env
      .path()
      .join(".agency")
      .join("worktrees")
      .join("1-delta-task")
      .exists()
  );

  Ok(())
}

#[test]
fn new_rejects_slugs_starting_with_digits() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("1invalid");
  cmd
    .assert()
    .failure()
    .stderr(predicates::str::contains("invalid slug: must start with a letter").from_utf8());

  Ok(())
}

#[test]
fn ps_lists_id_and_slug_in_order() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Create two tasks
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("alpha-task");
  cmd.assert().success();
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-edit").arg("beta-task");
  cmd.assert().success();

  // Run ps
  let mut cmd = env.bin_cmd()?;
  cmd.arg("ps");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::diff("ID SLUG\n 1 alpha-task\n 2 beta-task\n").from_utf8());

  Ok(())
}

#[test]
fn ps_handles_empty_state() -> Result<()> {
  let env = common::TestEnv::new();
  env.setup_git_repo()?;
  env.simulate_initial_commit()?;

  // Run ps with no tasks
  let mut cmd = env.bin_cmd()?;
  cmd.arg("ps");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::diff("ID SLUG\n").from_utf8());

  Ok(())
}
