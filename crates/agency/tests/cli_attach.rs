mod common;

use anyhow::Result;
use crate::common::test_env::TestEnv;

#[test]
fn attach_follow_conflicts_with_task_and_session() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env
      .agency()?
      .arg("attach")
      .arg("123")
      .arg("--follow")
      .assert()
      .failure();

    env
      .agency()?
      .arg("attach")
      .arg("--session")
      .arg("99")
      .arg("--follow")
      .assert()
      .failure();
    Ok(())
  })
}
