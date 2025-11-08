use std::fs;

use anyhow::Result;
use tempfile::Builder;

use agency::config::load_config;
mod common;

#[test]
fn defaults_only_provide_opencode_cmd() -> Result<()> {
  let dir = Builder::new().tempdir_in(common::tmp_root())?;
  // No global or project config
  let cfg = load_config(dir.path())?;
  let opencode = cfg.agents.get("opencode").expect("default opencode agent");
  assert_eq!(opencode.cmd, vec!["opencode", "--prompt", "$AGENCY_TASK"]);
  Ok(())
}

#[test]
fn global_override_takes_precedence_over_defaults() -> Result<()> {
  let dir = Builder::new().tempdir_in(common::tmp_root())?;
  let xdg_root = Builder::new().tempdir_in(common::tmp_root())?;
  temp_env::with_var(
    "XDG_CONFIG_HOME",
    Some(xdg_root.path().as_os_str()),
    || -> Result<()> {
      let global_dir = xdg_root.path().join("agency");
      fs::create_dir_all(&global_dir)?;
      let global_file = global_dir.join("agency.toml");
      fs::write(
        &global_file,
        r#"[agents.opencode]
cmd = ["oc", "-p", "$AGENCY_TASK"]
"#,
      )?;

      let cfg = load_config(dir.path())?;
      let opencode = cfg.agents.get("opencode").expect("opencode present");
      assert_eq!(opencode.cmd, vec!["oc", "-p", "$AGENCY_TASK"]);
      Ok(())
    },
  )?;
  Ok(())
}

#[test]
fn project_override_wins_over_global() -> Result<()> {
  let dir = Builder::new().tempdir_in(common::tmp_root())?;
  let xdg_root = Builder::new().tempdir_in(common::tmp_root())?;
  temp_env::with_var(
    "XDG_CONFIG_HOME",
    Some(xdg_root.path().as_os_str()),
    || -> Result<()> {
      // Global
      let global_dir = xdg_root.path().join("agency");
      fs::create_dir_all(&global_dir)?;
      fs::write(
        global_dir.join("agency.toml"),
        r#"[agents.opencode]
cmd = ["oc", "-p", "$AGENCY_TASK"]
"#,
      )?;

      // Project
      let project_dir = dir.path().join(".agency");
      fs::create_dir_all(&project_dir)?;
      fs::write(
        project_dir.join("agency.toml"),
        r#"[agents.opencode]
cmd = ["local", "--prompt", "$AGENCY_TASK"]
"#,
      )?;

      let cfg = load_config(dir.path())?;
      let opencode = cfg.agents.get("opencode").expect("opencode present");
      assert_eq!(opencode.cmd, vec!["local", "--prompt", "$AGENCY_TASK"]);
      Ok(())
    },
  )?;
  Ok(())
}

#[test]
fn missing_keys_default_to_empty() -> Result<()> {
  let dir = Builder::new().tempdir_in(common::tmp_root())?;
  let project_dir = dir.path().join(".agency");
  fs::create_dir_all(&project_dir)?;
  // Provide an empty table to ensure defaulting works
  fs::write(
    project_dir.join("agency.toml"),
    r"[agents.custom]
",
  )?;

  let cfg = load_config(dir.path())?;
  // Unknown keys preserved but ignored; ensure array defaults
  let custom = cfg.agents.get("custom").expect("custom agent present");
  assert!(custom.cmd.is_empty(), "cmd should default to empty array");
  Ok(())
}

#[test]
fn invalid_toml_fails_with_actionable_error() {
  let dir = Builder::new().tempdir_in(common::tmp_root()).expect("tmp");
  let project_dir = dir.path().join(".agency");
  fs::create_dir_all(&project_dir).expect("mkdir");
  fs::write(
    project_dir.join("agency.toml"),
    "[agents.opencode]\ncmd = [1, 2,",
  )
  .expect("write");

  let err = load_config(dir.path()).expect_err("should fail");
  let msg = err.to_string();
  assert!(
    msg.contains("invalid TOML"),
    "error should mention invalid TOML: {msg}"
  );
}
