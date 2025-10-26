use std::process::Command;
use std::time::Duration;

use anyhow::Result;
use assert_cmd::prelude::*; // cargo_bin()
use expectrl::Expect;
use expectrl::{Eof, Session};
use predicates::prelude::*; // predicates for readable assertions

#[test]
fn help_includes_core_sections() -> Result<()> {
  let mut cmd = Command::cargo_bin("agency")?;
  cmd.arg("--help");

  cmd.assert().success().stdout(
    predicates::str::contains("Usage")
      .and(predicates::str::contains("Options"))
      .and(predicates::str::contains("-h, --help"))
      .and(predicates::str::contains("-V, --version"))
      .trim()
      .from_utf8(),
  );

  Ok(())
}

#[test]
fn help_over_pty_shows_usage_and_exits() -> Result<()> {
  // Build the command to run the installed test binary with --help
  let mut cmd = Command::cargo_bin("agency")?;
  cmd.arg("--help");

  // Spawn it under a PTY to validate basic interactive behavior doesn't break output
  let mut session = Session::spawn(cmd)?;
  session.set_expect_timeout(Some(Duration::from_secs(2)));

  // Look for a stable anchor and then EOF
  session.expect("Usage")?;
  session.expect(Eof)?;

  Ok(())
}
