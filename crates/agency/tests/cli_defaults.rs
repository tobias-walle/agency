mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn defaults_prints_embedded_config() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env
      .agency()?
      .arg("defaults")
      .assert()
      .success()
      .stdout(predicates::str::contains("Embedded agency defaults").from_utf8())
      .stdout(predicates::str::contains("[agents.claude]").from_utf8());
    Ok(())
  })
}
