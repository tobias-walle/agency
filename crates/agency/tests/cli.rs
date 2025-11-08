mod common;

use anyhow::Result;
use gix as git;
use predicates::prelude::*;
use temp_env::with_vars;

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
    data.contains("base_branch: main\n"),
    "front matter should contain base_branch: main"
  );
  assert!(
    data.contains("\n---\n# Alpha Task\n"),
    "should close YAML and include new title without extra blank line"
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
  let repo = git::discover(env.path())?;
  let full = format!("refs/heads/{}", env.branch_name(id, &slug));
  assert!(repo.find_reference(&full).is_ok());
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
  let repo = git::discover(env.path())?;
  let full = format!("refs/heads/{}", env.branch_name(id, &slug));
  assert!(repo.find_reference(&full).is_err());
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

  let repo = git::discover(env.path())?;
  let full = format!("refs/heads/{}", env.branch_name(id2, &slug2));
  assert!(repo.find_reference(&full).is_ok());

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

  let repo = git::discover(env.path())?;
  let full = format!("refs/heads/{}", env.branch_name(id2, &slug2));
  assert!(repo.find_reference(&full).is_ok());

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

#[test]
fn new_bootstraps_git_ignored_root_files_with_defaults() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // Create .gitignore at repo root
  let gi = env.path().join(".gitignore");
  std::fs::write(
    &gi,
    ".env\n.env.local\nsecrets.txt\n.venv/\n.direnv/\nbig.bin\n",
  )?;

  // Create ignored root files and dirs
  std::fs::write(env.path().join(".env"), "KEY=VALUE\n")?;
  std::fs::write(env.path().join(".env.local"), "LOCAL=1\n")?;
  std::fs::write(env.path().join("secrets.txt"), "secret\n")?;
  // big.bin >= 10MB
  let big = env.path().join("big.bin");
  let f = std::fs::File::create(&big)?;
  f.set_len(10 * 1024 * 1024 + 1)?;
  // dirs
  std::fs::create_dir_all(env.path().join(".venv"))?;
  std::fs::write(env.path().join(".venv").join("pkg.txt"), "x\n")?;
  std::fs::create_dir_all(env.path().join(".direnv"))?;
  std::fs::write(env.path().join(".direnv").join("env.txt"), "x\n")?;

  // Create task
  let (id, slug) = env.new_task("bootstrap-a", &["--no-edit"])?;
  let wt = env.worktree_dir_path(id, &slug);

  // Files present (<=10MB)
  assert!(wt.join(".env").is_file());
  assert!(wt.join(".env.local").is_file());
  assert!(wt.join("secrets.txt").is_file());
  // Excluded by size; dirs are included by default now
  assert!(!wt.join("big.bin").exists());
  assert!(wt.join(".venv").is_dir());
  assert!(wt.join(".venv").join("pkg.txt").is_file());
  assert!(wt.join(".direnv").is_dir());
  assert!(wt.join(".direnv").join("env.txt").is_file());
  // Always excluded from copying
  assert!(!wt.join(".agency").exists());

  Ok(())
}

#[test]
fn new_bootstrap_respects_config_includes_and_excludes() -> Result<()> {
  let env = common::TestEnv::new();
  env.init_repo()?;

  // .gitignore
  std::fs::write(
    env.path().join(".gitignore"),
    ".env\n.env.local\nsecrets.txt\n.venv/\n.direnv/\n",
  )?;
  // Files/dirs
  std::fs::write(env.path().join(".env"), "KEY=VALUE\n")?;
  std::fs::write(env.path().join(".env.local"), "LOCAL=1\n")?;
  std::fs::write(env.path().join("secrets.txt"), "secret\n")?;
  std::fs::create_dir_all(env.path().join(".venv"))?;
  std::fs::write(env.path().join(".venv").join("pkg.txt"), "x\n")?;
  std::fs::create_dir_all(env.path().join(".direnv"))?;
  std::fs::write(env.path().join(".direnv").join("env.txt"), "x\n")?;

  // Project config: include .venv
  let proj_cfg_dir = env.path().join(".agency");
  std::fs::create_dir_all(&proj_cfg_dir)?;
  std::fs::write(
    proj_cfg_dir.join("agency.toml"),
    "[bootstrap]\ninclude=[\".venv\"]\n",
  )?;

  // XDG config: include .direnv, exclude .env.local
  let xdg_root = common::tmp_root().join("xdg-config");
  let agency_dir = xdg_root.join("agency");
  std::fs::create_dir_all(&agency_dir)?;
  std::fs::write(
    agency_dir.join("agency.toml"),
    "[bootstrap]\ninclude=[\".direnv\"]\nexclude=[\".env.local\"]\n",
  )?;

  // Scope XDG path only for this call
  with_vars(
    [("XDG_CONFIG_HOME", Some(xdg_root.display().to_string()))],
    || {
      let (id, slug) = env.new_task("bootstrap-b", &["--no-edit"]).unwrap();
      let wt = env.worktree_dir_path(id, &slug);
      assert!(wt.join(".env").is_file());
      assert!(wt.join("secrets.txt").is_file());
      assert!(wt.join(".venv").is_dir());
      assert!(wt.join(".venv").join("pkg.txt").is_file());
      assert!(wt.join(".direnv").is_dir());
      assert!(wt.join(".direnv").join("env.txt").is_file());
      // excluded via XDG override
      assert!(!wt.join(".env.local").exists());
      // always excluded from copying
      assert!(!wt.join(".agency").exists());
    },
  );

  Ok(())
}
