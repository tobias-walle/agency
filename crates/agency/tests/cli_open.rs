mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;

#[test]
fn open_opens_worktree_via_editor() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;
    let (id, _slug) = env.new_task("open-task", &["--draft"])?;

    env.with_env_vars(
      &[("EDITOR", Some("true".to_string()))],
      |env| -> Result<()> {
        env
          .agency()?
          .arg("open")
          .arg(id.to_string())
          .assert()
          .success();
        Ok(())
      },
    )?;

    Ok(())
  })
}
