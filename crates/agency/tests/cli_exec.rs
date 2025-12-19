mod common;

use anyhow::Result;
use predicates::prelude::*;

use crate::common::test_env::TestEnv;

#[test]
fn exec_runs_command_in_worktree() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("exec-test", &[])?;
    env.bootstrap_task(id)?;

    // Command should run in worktree, output "hello", exit 0
    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("echo")
      .arg("hello")
      .assert()
      .success()
      .stdout(predicate::str::contains("hello").from_utf8());

    Ok(())
  })
}

#[test]
fn exec_passes_exit_code() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("exit-test", &[])?;
    env.bootstrap_task(id)?;

    // 'false' command should exit with code 1
    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("false")
      .assert()
      .code(1);

    // Custom exit code
    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("sh")
      .arg("-c")
      .arg("exit 42")
      .assert()
      .code(42);

    Ok(())
  })
}

#[test]
fn exec_supports_double_dash_separator() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("dash-test", &[])?;
    env.bootstrap_task(id)?;

    // Should work with -- separator and handle flags
    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("--")
      .arg("ls")
      .arg("-la")
      .assert()
      .success();

    Ok(())
  })
}

#[test]
fn exec_sets_environment_variables() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("env-test", &[])?;
    env.bootstrap_task(id)?;

    // Check AGENCY_TASK_ID is set
    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("sh")
      .arg("-c")
      .arg("echo $AGENCY_TASK_ID")
      .assert()
      .success()
      .stdout(predicate::str::contains(id.to_string()).from_utf8());

    Ok(())
  })
}

#[test]
fn exec_fails_without_worktree() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("no-wt-test", &[])?;
    // Don't bootstrap - no worktree exists

    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("echo")
      .arg("hi")
      .assert()
      .failure()
      .stderr(predicate::str::contains("worktree not found").from_utf8());

    Ok(())
  })
}

#[test]
fn exec_fails_for_nonexistent_task() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("exec")
      .arg("999")
      .arg("echo")
      .arg("hi")
      .assert()
      .failure();

    Ok(())
  })
}

#[test]
fn exec_runs_in_worktree_directory() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("cwd-test", &[])?;
    env.bootstrap_task(id)?;

    let expected_wt = env.worktree_dir_path(id, &slug);

    // pwd should output the worktree path
    env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("pwd")
      .assert()
      .success()
      .stdout(predicate::str::contains(expected_wt.display().to_string()).from_utf8());

    Ok(())
  })
}

#[test]
fn exec_no_agency_logs_in_output() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("quiet-test", &[])?;
    env.bootstrap_task(id)?;

    // Output should be clean - no agency prefixes/logs
    let output = env
      .agency()?
      .arg("exec")
      .arg(id.to_string())
      .arg("echo")
      .arg("clean output")
      .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "clean output");

    Ok(())
  })
}

#[test]
fn exec_works_with_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("slug-test", &[])?;
    env.bootstrap_task(id)?;

    // Should work with slug instead of id
    env
      .agency()?
      .arg("exec")
      .arg(&slug)
      .arg("echo")
      .arg("by-slug")
      .assert()
      .success()
      .stdout(predicate::str::contains("by-slug").from_utf8());

    Ok(())
  })
}
