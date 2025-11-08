mod common;

use anyhow::Result;
use predicates::prelude::*;

#[test]
fn new_creates_markdown_branch_and_worktree() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create task
  let (id, slug) = env.new_task("alpha-task", &["--no-edit"])?;

  // Check markdown
  let file = env.task_file_path(id, &slug);
  assert!(
    file.is_file(),
    "task file should exist at {}",
    file.display()
  );

  // Check branch and worktree
  assert!(env.branch_exists(id, &slug)?);
  let wt_dir = env.worktree_dir_path(id, &slug);
  assert!(
    wt_dir.is_dir(),
    "worktree dir should exist at {}",
    wt_dir.display()
  );

  Ok(())
}

#[test]
fn new_accepts_no_attach_flag() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Run without helper to ensure the flag is accepted
  let mut cmd = env.bin_cmd()?;
  cmd.arg("new").arg("--no-attach").arg("epsilon-task");
  cmd.assert().success();

  Ok(())
}

#[test]
fn new_writes_yaml_header_when_agent_specified() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("alpha-task", &["--no-edit", "-a", "fake"])?;
  // Check markdown content includes YAML front matter
  let file = env.task_file_path(id, &slug);
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
    data.contains(&format!("\n---\n\n# Task {id}: {slug}\n")),
    "should close YAML and include title"
  );

  Ok(())
}

#[test]
fn path_prints_absolute_worktree_path_by_id_and_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("beta-task", &["--no-edit"])?;

  let expected = env.worktree_dir_path(id, &slug);
  let expected_canon = expected.canonicalize().unwrap_or(expected.clone());
  let expected_str = expected_canon.display().to_string() + "\n";

  // path by id
  let mut cmd = env.bin_cmd()?;
  cmd.arg("path").arg(id.to_string());
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(expected_str.clone()).from_utf8());

  // path by slug
  let mut cmd = env.bin_cmd()?;
  cmd.arg("path").arg(&slug);
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(expected_str).from_utf8());

  Ok(())
}

#[test]
fn branch_prints_branch_name_by_id_and_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("gamma-task", &["--no-edit"])?;

  // by id
  let mut cmd = env.bin_cmd()?;
  cmd.arg("branch").arg(id.to_string());
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(env.branch_name(id, &slug)).from_utf8());

  // by slug
  let mut cmd = env.bin_cmd()?;
  cmd.arg("branch").arg(&slug);
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains(env.branch_name(id, &slug)).from_utf8());

  Ok(())
}

#[test]
fn rm_confirms_and_removes_on_y_or_y() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id, slug) = env.new_task("delta-task", &["--no-edit"])?;

  // Run rm and cancel (pipe stdin via assert_cmd)
  let mut cmd = env.bin_cmd()?;
  cmd
    .arg("rm")
    .arg(id.to_string())
    .write_stdin("n\n")
    .assert()
    .success()
    .stdout(predicates::str::contains("Cancelled").from_utf8());

  // Ensure still present
  let repo = git2::Repository::discover(env.path())?;
  assert!(
    repo
      .find_branch(&env.branch_name(id, &slug), git2::BranchType::Local)
      .is_ok()
  );
  assert!(env.task_file_path(id, &slug).is_file());
  assert!(env.worktree_dir_path(id, &slug).is_dir());

  // Run rm and confirm with Y (pipe stdin via assert_cmd)
  let mut cmd = env.bin_cmd()?;
  cmd
    .arg("rm")
    .arg(&slug)
    .write_stdin("Y\n")
    .assert()
    .success()
    .stdout(predicates::str::contains("Removed task, branch, and worktree").from_utf8());

  // Verify removal
  let repo = git2::Repository::discover(env.path())?;
  assert!(
    repo
      .find_branch(&env.branch_name(id, &slug), git2::BranchType::Local)
      .is_err()
  );
  assert!(!env.task_file_path(id, &slug).exists());
  assert!(!env.worktree_dir_path(id, &slug).exists());

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
fn new_auto_suffixes_duplicate_slug_to_slug2() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (_id1, _slug1) = env.new_task("alpha", &["--no-edit"])?;
  let (id2, slug2) = env.new_task("alpha", &["--no-edit"])?;

  // Check file, branch, worktree for the second task
  let file = env.task_file_path(id2, &slug2);
  assert!(file.is_file());

  let repo = git2::Repository::discover(env.path())?;
  assert!(
    repo
      .find_branch(&env.branch_name(id2, &slug2), git2::BranchType::Local)
      .is_ok()
  );

  let wt_dir = env.worktree_dir_path(id2, &slug2);
  assert!(wt_dir.is_dir());

  Ok(())
}

#[test]
fn new_increments_trailing_number_slug() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (_id1, _slug1) = env.new_task("alpha2", &["--no-edit"])?;
  let (id2, slug2) = env.new_task("alpha2", &["--no-edit"])?;

  // Check artifacts for the second task
  let file = env.task_file_path(id2, &slug2);
  assert!(file.is_file());

  let repo = git2::Repository::discover(env.path())?;
  assert!(
    repo
      .find_branch(&env.branch_name(id2, &slug2), git2::BranchType::Local)
      .is_ok()
  );

  let wt_dir = env.worktree_dir_path(id2, &slug2);
  assert!(wt_dir.is_dir());

  Ok(())
}

#[test]
fn ps_lists_id_and_slug_in_order() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;
  let (id1, slug1) = env.new_task("alpha-task", &["--no-edit"])?;
  let (id2, slug2) = env.new_task("beta-task", &["--no-edit"])?;

  // Run ps
  let mut cmd = env.bin_cmd()?;
  cmd.arg("ps");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("ID SLUG\n").from_utf8())
    .stdout(predicates::str::contains(format!(" {id1} {slug1}\n")).from_utf8())
    .stdout(predicates::str::contains(format!(" {id2} {slug2}\n")).from_utf8());

  Ok(())
}

#[test]
fn ps_handles_empty_state() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Run ps with no tasks
  let mut cmd = env.bin_cmd()?;
  cmd.arg("ps");
  cmd
    .assert()
    .success()
    .stdout(predicates::str::contains("ID SLUG\n").from_utf8());

  Ok(())
}
