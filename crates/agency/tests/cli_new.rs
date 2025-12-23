mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn new_creates_markdown_branch_and_worktree() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("alpha-task", &[])?;

    let file = env.task_file_path(id, &slug);
    assert!(
      file.is_file(),
      "task file should exist at {}",
      file.display()
    );

    assert!(!env.branch_exists(id, &slug)?);
    let wt_dir = env.worktree_dir_path(id, &slug);
    assert!(!wt_dir.exists());

    env.bootstrap_task(id)?;
    assert!(env.branch_exists(id, &slug)?);
    assert!(wt_dir.is_dir());

    Ok(())
  })
}

#[test]
fn new_persists_description_when_provided() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task(
      "desc-task",
      &["--draft", "--description", "Automated test body"],
    )?;
    let file = env.task_file_path(id, &slug);
    let data = std::fs::read_to_string(&file)?;
    assert!(data.contains("Automated test body"));
    Ok(())
  })
}

#[test]
fn new_accepts_draft_flag() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("new")
      .arg("--draft")
      .arg("epsilon-task")
      .assert()
      .success();

    Ok(())
  })
}

#[test]
fn new_runs_default_bootstrap_cmd_when_present() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let agen_dir = env.path().join(".agency");
    std::fs::create_dir_all(&agen_dir)?;
    let script = agen_dir.join("setup.sh");
    env.write_executable_script(
      &script,
      "#!/usr/bin/env bash\n\n echo bootstrap-script-output && echo ok > boot.out\n",
    )?;

    let (id, slug) = env.new_task("boot-cmd", &["--draft"])?;
    env.bootstrap_task(id)?;
    let wt = env.worktree_dir_path(id, &slug);
    assert!(wt.join("boot.out").is_file());

    Ok(())
  })
}

#[test]
fn new_skips_default_bootstrap_when_missing() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    std::fs::create_dir_all(env.path().join(".agency"))?;

    let (id, slug) = env.new_task("no-boot-script", &[])?;
    env.bootstrap_task(id)?;
    let wt = env.worktree_dir_path(id, &slug);
    assert!(!wt.join("boot.out").exists());
    Ok(())
  })
}

#[test]
fn new_supports_placeholder_root_in_bootstrap_cmd() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let agen_dir = env.path().join(".agency");
    std::fs::create_dir_all(&agen_dir)?;
    std::fs::write(
      agen_dir.join("agency.toml"),
      r#"
[bootstrap]
cmd=["bash","-lc","echo <root> > root.txt"]
"#
      .trim(),
    )?;

    let (id, slug) = env.new_task("boot-root", &[])?;
    env.bootstrap_task(id)?;
    let wt = env.worktree_dir_path(id, &slug);
    let data = std::fs::read_to_string(wt.join("root.txt"))?;
    let expect_root = env
      .path()
      .canonicalize()
      .unwrap_or_else(|_| env.path().to_path_buf())
      .display()
      .to_string();
    assert_eq!(data.trim(), expect_root);
    Ok(())
  })
}

#[test]
fn new_writes_yaml_header_when_agent_specified() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("alpha-task", &["-a", "sh", "--description", ""])?;
    let file = env.task_file_path(id, &slug);
    let data = std::fs::read_to_string(&file)?;
    assert!(
      data.starts_with("---\n"),
      "file should start with YAML '---' block"
    );
    assert!(
      data.contains("agent: sh\n"),
      "front matter should contain agent: sh"
    );
    assert!(
      data.contains("base_branch: main\n"),
      "front matter should contain base_branch: main"
    );
    assert_eq!(
      data, "---\nagent: sh\nbase_branch: main\n---\n",
      "task should contain only front matter when no description"
    );

    Ok(())
  })
}

#[test]
fn new_rejects_slugs_starting_with_digits() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.setup_git_repo()?;
    env.simulate_initial_commit()?;

    env
      .agency()?
      .arg("new")
      .arg("1invalid")
      .assert()
      .failure()
      .stderr(predicates::str::contains("invalid slug: must start with a letter").from_utf8());

    Ok(())
  })
}

#[test]
fn new_auto_suffixes_duplicate_slug_to_slug2() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (_id1, _slug1) = env.new_task("alpha", &[])?;
    let (id2, slug2) = env.new_task("alpha", &[])?;

    let file = env.task_file_path(id2, &slug2);
    assert!(file.is_file());

    Ok(())
  })
}

#[test]
fn new_increments_trailing_number_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (_id1, _slug1) = env.new_task("alpha2", &[])?;
    let (id2, slug2) = env.new_task("alpha2", &[])?;

    let file = env.task_file_path(id2, &slug2);
    assert!(file.is_file());

    Ok(())
  })
}

#[test]
fn new_uses_worktree_branch_as_base() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    // Create a feature branch and worktree
    env.git_create_branch("feature")?;
    let worktree_path = env.path().join("worktrees").join("feature-wt");
    env.git_stdout(&["worktree", "add", worktree_path.to_str().unwrap(), "feature"])?;

    // Run agency new from within the worktree (via current_dir)
    let output = env.agency()?
      .current_dir(&worktree_path)
      .args(["new", "test-task", "--draft", "--description", "test"])
      .output()?;

    assert!(output.status.success());

    // Read task file and verify base_branch is "feature"
    let (id, slug) = TestEnv::parse_new_task_output(&output.stdout)?;
    let task_content = env.read_task_file(id, &slug)?;
    assert!(task_content.contains("base_branch: feature"));

    // CRITICAL: Verify task is stored in main repo's .agency folder, NOT in worktree
    let task_file = env.task_file_path(id, &slug);
    assert!(task_file.starts_with(env.path())); // Main repo path
    assert!(!task_file.starts_with(&worktree_path)); // NOT in worktree
    assert!(task_file.exists());

    Ok(())
  })
}

#[test]
fn new_uses_main_repo_branch_when_in_main_repo() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    env.git_create_branch("develop")?;
    env.git_checkout("develop")?;

    let (id, slug) = env.new_task("test-task", &["--draft", "--description", "test"])?;
    let task_content = env.read_task_file(id, &slug)?;
    assert!(task_content.contains("base_branch: develop"));

    Ok(())
  })
}

#[test]
fn new_falls_back_to_main_on_detached_head_in_worktree() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    // Create worktree
    env.git_create_branch("feature")?;
    let worktree_path = env.path().join("worktrees").join("feature-wt");
    env.git_stdout(&["worktree", "add", worktree_path.to_str().unwrap(), "feature"])?;

    // Detach HEAD in worktree
    let _ = std::process::Command::new("git")
      .current_dir(&worktree_path)
      .args(["checkout", "--detach"])
      .status()?;

    // Run agency new from detached worktree
    let output = env.agency()?
      .current_dir(&worktree_path)
      .args(["new", "test-task", "--draft", "--description", "test"])
      .output()?;

    assert!(output.status.success());

    let (id, slug) = TestEnv::parse_new_task_output(&output.stdout)?;
    let task_content = env.read_task_file(id, &slug)?;
    assert!(task_content.contains("base_branch: main"));

    Ok(())
  })
}

#[test]
fn new_falls_back_to_main_on_detached_head_in_main_repo() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    env.git_checkout_detach()?;

    let (id, slug) = env.new_task("test-task", &["--draft", "--description", "test"])?;
    let task_content = env.read_task_file(id, &slug)?;
    assert!(task_content.contains("base_branch: main"));

    Ok(())
  })
}
