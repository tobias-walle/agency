mod common;

use anyhow::Result;
use crate::common::test_env::TestEnv;

#[test]
fn complete_marks_status_completed_and_uses_env() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    let (id, slug) = env.new_task("complete-a", &["--draft"])?;

    env
      .agency()?
      .arg("complete")
      .arg(id.to_string())
      .assert()
      .success();

    let flag = env
      .path()
      .join(".agency")
      .join("state")
      .join("completed")
      .join(format!("{id}-{slug}"));
    assert!(
      flag.is_file(),
      "completed flag should exist at {}",
      flag.display()
    );

    let (id2, slug2) = env.new_task("complete-b", &["--draft"])?;

    env
      .agency()?
      .arg("complete")
      .env("AGENCY_TASK_ID", id2.to_string())
      .assert()
      .success();

    let flag2 = env
      .path()
      .join(".agency")
      .join("state")
      .join("completed")
      .join(format!("{id2}-{slug2}"));
    assert!(
      flag2.is_file(),
      "completed flag should exist at {}",
      flag2.display()
    );

    Ok(())
  })
}

#[test]
fn reset_clears_completed_status() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, slug) = env.new_task("complete-reset", &["--draft"])?;

    env
      .agency()?
      .arg("complete")
      .arg(id.to_string())
      .assert()
      .success();

    env
      .agency()?
      .arg("reset")
      .arg(id.to_string())
      .assert()
      .success();

    let flag = env
      .path()
      .join(".agency")
      .join("state")
      .join("completed")
      .join(format!("{id}-{slug}"));
    assert!(
      !flag.exists(),
      "completed flag should be removed after reset: {}",
      flag.display()
    );

    Ok(())
  })
}
