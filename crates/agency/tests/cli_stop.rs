mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn stop_fails_when_daemon_not_running() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, _slug) = env.new_task("test-task", &[])?;

    env
      .agency()?
      .arg("stop")
      .arg(id.to_string())
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("Daemon not running")
          .or(predicates::str::contains("Failed to connect"))
          .or(predicates::str::contains("Connection refused"))
          .or(predicates::str::contains("No such file"))
          .from_utf8()
      );

    Ok(())
  })
}

#[test]
fn stop_accepts_task_slug() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (_id, slug) = env.new_task("slug-stop-test", &[])?;

    // Should fail with daemon not running, but should accept slug
    env
      .agency()?
      .arg("stop")
      .arg(&slug)
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("Daemon not running")
          .or(predicates::str::contains("Failed to connect"))
          .from_utf8()
      );

    Ok(())
  })
}

#[test]
fn stop_accepts_task_id() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, _slug) = env.new_task("id-stop-test", &[])?;

    // Should fail with daemon not running, but should accept id
    env
      .agency()?
      .arg("stop")
      .arg(id.to_string())
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("Daemon not running")
          .or(predicates::str::contains("Failed to connect"))
          .from_utf8()
      );

    Ok(())
  })
}

#[test]
fn stop_accepts_session_id_flag() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    // Should fail with daemon not running, but should accept --session flag
    env
      .agency()?
      .arg("stop")
      .arg("--session")
      .arg("999")
      .assert()
      .failure()
      .stderr(
        predicates::str::contains("Daemon not running")
          .or(predicates::str::contains("Failed to connect"))
          .from_utf8()
      );

    Ok(())
  })
}
