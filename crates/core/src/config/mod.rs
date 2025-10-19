mod defaults;
mod load;
pub mod paths;
pub mod types;
mod validate;
pub mod write;

pub use load::load;
pub use paths::{global_config_path, project_config_path, resolve_socket_path};
pub use types::{AgentConfig, Config, ConfigError, LogLevel, PtyConfig, Result};
pub use write::write_default_project_config;

#[cfg(test)]
pub(crate) use load::load_from_paths;
#[cfg(test)]
pub(crate) use paths::resolve_socket_path_for;

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[test]
  fn defaults_are_correct() {
    let cfg = Config::default();
    assert_eq!(cfg.log_level, LogLevel::Info);
    assert_eq!(cfg.idle_timeout_secs, 10);
    assert_eq!(cfg.dwell_secs, 2);
    assert_eq!(cfg.concurrency, None);
    assert!(!cfg.confirm_by_default);
    assert_eq!(cfg.default_agent, None);
    assert_eq!(cfg.pty.detach_keys, None);
    let opencode = cfg.agents.get("opencode").expect("opencode agent");
    assert_eq!(
      opencode.start,
      vec![
        "opencode".to_string(),
        "--agent".to_string(),
        "plan".to_string(),
        "-p".to_string(),
        "$AGENCY_PROMPT".to_string()
      ]
    );
    assert_eq!(opencode.display_name.as_deref(), Some("OpenCode"));
    let fake = cfg.agents.get("fake").expect("fake agent");
    assert_eq!(fake.start, vec!["sh".to_string()]);
    assert_eq!(fake.display_name.as_deref(), Some("Shell"));
  }

  #[test]
  fn merge_precedence_project_overrides_global_over_defaults() {
    let td = tempfile::tempdir().unwrap();
    let global = td.path().join("global.toml");
    let project = td.path().join("project.toml");

    fs::write(
      &global,
      r#"
log_level = "warn"
idle_timeout_secs = 5
confirm_by_default = false
default_agent = "opencode"
[pty]
detach_keys = "ctrl-p"

[agents.opencode]
start = ["opencode", "--agent", "plan", "-p", "GLOBAL"]
"#,
    )
    .unwrap();

    fs::write(
      &project,
      r#"
log_level = "debug"
dwell_secs = 3
default_agent = "fake"
[pty]
detach_keys = "ctrl-q"

[agents.opencode]
start = ["opencode", "--agent", "plan", "-p", "PROJECT"]

[agents.fake]
start = ["sh", "-c", "echo project"]
"#,
    )
    .unwrap();

    let cfg = load_from_paths(Some(&global), Some(&project)).unwrap();
    // project overrides global
    assert_eq!(cfg.log_level, LogLevel::Debug);
    // global overrides default
    assert_eq!(cfg.idle_timeout_secs, 5);
    // project adds value
    assert_eq!(cfg.dwell_secs, 3);
    // global changed default
    assert!(!cfg.confirm_by_default);
    // default_agent precedence: project overrides global
    assert_eq!(cfg.default_agent, Some(crate::domain::task::Agent::Fake));
    // pty precedence
    assert_eq!(cfg.pty.detach_keys.as_deref(), Some("ctrl-q"));

    let opencode = cfg.agents.get("opencode").expect("opencode agent");
    assert_eq!(
      opencode.start,
      vec![
        "opencode".to_string(),
        "--agent".to_string(),
        "plan".to_string(),
        "-p".to_string(),
        "PROJECT".to_string()
      ]
    );
    let fake = cfg.agents.get("fake").expect("fake agent");
    assert_eq!(
      fake.start,
      vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo project".to_string()
      ]
    );
  }

  #[test]
  fn missing_default_agent_definition_is_rejected() {
    let td = tempfile::tempdir().unwrap();
    let project = td.path().join("project.toml");

    fs::write(
      &project,
      r#"
default_agent = "claude-code"
"#,
    )
    .unwrap();

    let err = load_from_paths(None, Some(&project)).unwrap_err();
    match err {
      ConfigError::MissingAgentDefinition { agent } => assert_eq!(agent, "claude-code"),
      other => panic!("unexpected error: {:?}", other),
    }
  }

  #[test]
  fn empty_start_list_is_invalid() {
    let td = tempfile::tempdir().unwrap();
    let project = td.path().join("project.toml");

    fs::write(
      &project,
      r#"
[agents.fake]
start = []
"#,
    )
    .unwrap();

    let err = load_from_paths(None, Some(&project)).unwrap_err();
    match err {
      ConfigError::InvalidAgentDefinition { agent } => assert_eq!(agent, "fake"),
      other => panic!("unexpected error: {:?}", other),
    }
  }

  #[test]
  fn socket_env_overrides() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("sock");
    let got = resolve_socket_path_for(Some(p.clone())).unwrap();
    assert_eq!(got, p);
  }

  #[test]
  fn socket_platform_fallback() {
    let got = resolve_socket_path_for(None).unwrap();

    let expected = if let Some(r) = dirs::runtime_dir() {
      r.join("agency.sock")
    } else if let Some(d) = dirs::data_dir() {
      d.join("agency.sock")
    } else {
      panic!("No runtime_dir() or data_dir() available on this platform for test");
    };

    assert_eq!(
      got, expected,
      "Socket path should prefer runtime_dir() and fall back to data_dir()"
    );
  }
}
