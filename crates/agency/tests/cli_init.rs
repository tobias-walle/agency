mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn init_scaffolds_files_after_confirmation() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    let root = env.path().to_path_buf();
    env
      .agency()?
      .arg("init")
      .write_stdin("y\n")
      .assert()
      .success()
      .stdout(predicates::str::contains(".agency/agency.toml").from_utf8())
      .stdout(predicates::str::contains(".agency/setup.sh").from_utf8())
      .stdout(predicates::str::contains(".gitignore").from_utf8());

    let agency_dir = root.join(".agency");
    assert!(agency_dir.is_dir(), ".agency directory should be created");
    let cfg = agency_dir.join("agency.toml");
    assert!(cfg.is_file(), "empty agency.toml should be created");
    let script = agency_dir.join("setup.sh");
    assert!(script.is_file(), "bootstrap script should be created");
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      let perms = std::fs::metadata(&script)?.permissions();
      assert!(
        perms.mode() & 0o111 != 0,
        "setup.sh should be executable: mode {:o}",
        perms.mode()
      );
    }
    let script_body = std::fs::read_to_string(&script)?;
    assert!(
      script_body.contains("#!/usr/bin/env bash"),
      "script should include shebang for editing: {script_body}"
    );
    Ok(())
  })
}

#[test]
fn init_appends_gitignore_when_missing() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    let gi = env.path().join(".gitignore");
    assert!(!gi.exists(), ".gitignore should not exist before init");

    env
      .agency()?
      .arg("init")
      .write_stdin("y\n")
      .assert()
      .success();

    let contents = std::fs::read_to_string(&gi)?;
    assert_eq!(
      contents, ".agency/*\n!.agency/setup.sh\n",
      ".gitignore should contain agency entries"
    );
    Ok(())
  })
}

#[test]
fn init_skips_gitignore_when_agency_entry_exists() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    let gi = env.path().join(".gitignore");
    std::fs::write(&gi, "# existing\n.agency\n")?;

    env
      .agency()?
      .arg("init")
      .write_stdin("y\n")
      .assert()
      .success();

    let contents = std::fs::read_to_string(&gi)?;
    assert_eq!(
      contents, "# existing\n.agency\n",
      ".gitignore should remain unchanged when .agency entry exists"
    );
    Ok(())
  })
}

#[test]
fn init_sets_agent_config() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    env
      .agency()?
      .arg("init")
      .arg("--agent")
      .arg("claude")
      .write_stdin("y\n")
      .assert()
      .success();

    let cfg_path = env.path().join(".agency").join("agency.toml");
    let contents = std::fs::read_to_string(cfg_path)?;
    assert!(
      contents.contains("agent = \"claude\""),
      "agency.toml should contain the specified agent"
    );
    Ok(())
  })
}

#[test]
fn init_updates_existing_agent_config() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    let agency_dir = env.path().join(".agency");
    std::fs::create_dir_all(&agency_dir)?;
    let cfg_path = agency_dir.join("agency.toml");
    std::fs::write(&cfg_path, "# some comment\nother = \"value\"\nagent = \"old\"\n")?;

    env
      .agency()?
      .arg("init")
      .arg("--agent")
      .arg("new-agent")
      .write_stdin("y\n")
      .assert()
      .success();

    let contents = std::fs::read_to_string(cfg_path)?;
    assert!(contents.contains("# some comment"), "Comments should be preserved");
    assert!(contents.contains("other = \"value\""), "Other keys should be preserved");
    assert!(contents.contains("agent = \"new-agent\""), "Agent should be updated");
    assert!(!contents.contains("agent = \"old\""), "Old agent should be gone");
    Ok(())
  })
}
