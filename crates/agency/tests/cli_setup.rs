mod common;

use crate::common::test_env::TestEnv;
use anyhow::Result;
use predicates::prelude::*;

#[test]
fn setup_creates_global_config_via_wizard() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    let config_home = env.xdg_home_dir().to_path_buf();
    // Initialize a git repo so agency doesn't find the parent agency repo
    env.git().args(["init"]).status()?;
    env.git().args(["config", "user.email", "test@test.com"]).status()?;
    env.git().args(["config", "user.name", "Test"]).status()?;
    env.git().args(["commit", "--allow-empty", "-m", "init"]).status()?;
    env.add_xdg_home_bin("claude", "#!/usr/bin/env bash\nexit 0\n")?;

    env
      .agency()?
      .arg("setup")
      .write_stdin("claude\n\n\n")
      .assert()
      .success()
      .stdout(predicates::str::contains("agency defaults").from_utf8())
      .stdout(predicates::str::contains("agency init").from_utf8());

    let cfg_file = config_home.join("agency").join("agency.toml");
    assert!(
      cfg_file.is_file(),
      "setup should write global config at {}",
      cfg_file.display()
    );
    let data = std::fs::read_to_string(&cfg_file)?;
    assert!(
      data.contains("agent = \"claude\""),
      "config should record selected agent: {data}"
    );
    assert!(
      !data.contains("keybindings"),
      "no keybinding overrides present by default: {data}"
    );
    assert!(
      data.contains("shell = [\"/bin/sh\"]"),
      "shell default should be pinned when SHELL is unset: {data}"
    );
    // Check that no uncommented editor line exists (template has commented # editor = ...)
    let has_editor_setting = data
      .lines()
      .any(|line| line.trim_start().starts_with("editor ="));
    assert!(
      !has_editor_setting,
      "default editor should not create override: {data}"
    );
    Ok(())
  })
}

#[test]
fn setup_updates_existing_config_and_warns() -> Result<()> {
  TestEnv::run(|env| -> Result<()> {
    let config_home = env.xdg_home_dir().to_path_buf();
    let agency_dir = config_home.join("agency");
    std::fs::create_dir_all(&agency_dir)?;
    let cfg = agency_dir.join("agency.toml");
    std::fs::write(
      &cfg,
      r#"agent = "claude"
[bootstrap]
include = ["scripts"]
"#,
    )?;

    env
      .agency()?
      .arg("setup")
      .write_stdin("opencode\nzsh\n\n")
      .assert()
      .success()
      .stdout(predicates::str::contains("existing config").from_utf8());

    let data = std::fs::read_to_string(&cfg)?;
    assert!(
      data.contains("agent = \"opencode\""),
      "agent should be updated: {data}"
    );
    assert!(
      data.contains("shell = [\"zsh\"]"),
      "shell command override should be persisted: {data}"
    );
    assert!(
      data.contains("[bootstrap]"),
      "unrelated keys must be preserved when rewriting config: {data}"
    );
    // Check that no uncommented editor line exists
    let has_editor_setting = data
      .lines()
      .any(|line| line.trim_start().starts_with("editor ="));
    assert!(
      !has_editor_setting,
      "default editor should not create override: {data}"
    );
    Ok(())
  })
}
