mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn sessions_bails_when_daemon_not_running() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env.init_repo()?;

    env.agency()?.arg("sessions").assert().failure().stderr(
      predicates::str::contains("Daemon not running. Please start it with `agency daemon start`")
        .from_utf8(),
    );

    Ok(())
  })
}
