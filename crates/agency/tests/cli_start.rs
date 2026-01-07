mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn start_fails_with_invalid_task_id() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("start")
      .arg("999")
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("not found")
          .or(predicates::str::contains("No such"))
          .from_utf8()
      );

    Ok(())
  })
}

#[test]
fn start_accepts_task_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    if !env.sockets_available() {
      eprintln!("Skipping start_accepts_task_slug: Unix sockets not available in sandbox");
      return Ok(());
    }

    let (_id, slug) = env.new_task("start-test", &[])?;

    env.agency_daemon_start()?;

    // This will likely fail due to missing tmux or other deps,
    // but should at least accept the slug and attempt to start
    let output = env
      .agency()?
      .arg("start")
      .arg(&slug)
      .output()?;

    // Should either succeed or fail with a reasonable error
    // (not "task not found")
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
      !stderr.contains("Task not found") && !stderr.contains("No such task"),
      "Should accept slug, but got: {}", stderr
    );

    env.agency_daemon_stop()?;

    Ok(())
  })
}

#[test]
fn start_fails_when_daemon_not_running() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, _slug) = env.new_task("daemon-check", &[])?;

    env
      .agency()?
      .arg("start")
      .arg(id.to_string())
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("Failed to connect")
          .or(predicates::str::contains("Connection refused"))
          .or(predicates::str::contains("No such file"))
          .or(predicates::str::contains("Daemon not running"))
          .from_utf8()
      );

    Ok(())
  })
}
