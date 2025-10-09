use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use dirs::data_dir;
use dirs::runtime_dir;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Log level for the daemon and CLI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
  Off,
  Warn,
  #[default]
  Info,
  Debug,
  Trace,
}

/// PTY-related configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PtyConfig {
  /// Detach sequence as a comma-separated list like "ctrl-q" or "ctrl-p,ctrl-q".
  /// None means use the built-in default (Ctrl-Q) at use sites.
  pub detach_keys: Option<String>,
}

/// Configuration for launching an agent process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfig {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub display_name: Option<String>,
  pub start: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub resume: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub run: Option<Vec<String>>,
}

/// Effective configuration after merging defaults, global, and project config
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
  pub log_level: LogLevel,
  /// Idle timeout in seconds (defaults to 10)
  pub idle_timeout_secs: u64,
  /// Dwell time in seconds (defaults to 2)
  pub dwell_secs: u64,
  /// Max concurrent tasks (None means unlimited)
  pub concurrency: Option<usize>,
  /// Default answer for confirmation prompts for destructive commands; false = default "No"
  pub confirm_by_default: bool,
  /// Default agent to use for new tasks (opencode | claude-code | fake)
  #[serde(default)]
  pub default_agent: Option<crate::domain::task::Agent>,
  /// PTY configuration
  pub pty: PtyConfig,
  /// Agent command definitions resolved by the daemon when spawning tasks.
  pub agents: BTreeMap<String, AgentConfig>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      log_level: LogLevel::Info,
      idle_timeout_secs: 10,
      dwell_secs: 2,
      concurrency: None,
      confirm_by_default: false,
      default_agent: None,
      pty: PtyConfig::default(),
      agents: builtin_agents(),
    }
  }
}

fn builtin_agents() -> BTreeMap<String, AgentConfig> {
  let mut agents = BTreeMap::new();
  agents.insert(
    "opencode".to_string(),
    AgentConfig {
      display_name: Some("OpenCode".to_string()),
      start: vec![
        "opencode".to_string(),
        "--agent".to_string(),
        "plan".to_string(),
        "-p".to_string(),
        "$AGENCY_PROMPT".to_string(),
      ],
      resume: None,
      run: None,
    },
  );
  agents.insert(
    "fake".to_string(),
    AgentConfig {
      display_name: Some("Shell".to_string()),
      start: vec!["sh".to_string()],
      resume: None,
      run: None,
    },
  );
  agents
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct PartialPtyConfig {
  /// Use Option<Option<String>> to distinguish missing vs explicit empty/null in future, even
  /// though TOML has no null literal. Missing means keep base; Some(v) overrides.
  pub detach_keys: Option<String>,
}

impl PartialPtyConfig {
  fn merge_over(self, base: PtyConfig) -> PtyConfig {
    PtyConfig {
      detach_keys: self.detach_keys.or(base.detach_keys),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct PartialConfig {
  pub log_level: Option<LogLevel>,
  pub idle_timeout_secs: Option<u64>,
  pub dwell_secs: Option<u64>,
  pub concurrency: Option<Option<usize>>, // Some(None) means explicit null, None means not provided
  pub confirm_by_default: Option<bool>,
  pub default_agent: Option<crate::domain::task::Agent>,
  pub pty: Option<PartialPtyConfig>,
  pub agents: Option<BTreeMap<String, AgentConfig>>,
}

impl PartialConfig {
  fn merge_over(self, base: Config) -> Config {
    let PartialConfig {
      log_level,
      idle_timeout_secs,
      dwell_secs,
      concurrency,
      confirm_by_default,
      default_agent,
      pty,
      agents,
    } = self;

    let Config {
      log_level: base_log_level,
      idle_timeout_secs: base_idle_timeout_secs,
      dwell_secs: base_dwell_secs,
      concurrency: base_concurrency,
      confirm_by_default: base_confirm_by_default,
      default_agent: base_default_agent,
      pty: base_pty,
      agents: base_agents,
    } = base;

    let mut merged_agents = base_agents;
    if let Some(overrides) = agents {
      for (name, cfg) in overrides {
        merged_agents.insert(name, cfg);
      }
    }

    Config {
      log_level: log_level.unwrap_or(base_log_level),
      idle_timeout_secs: idle_timeout_secs.unwrap_or(base_idle_timeout_secs),
      dwell_secs: dwell_secs.unwrap_or(base_dwell_secs),
      concurrency: concurrency.unwrap_or(base_concurrency),
      confirm_by_default: confirm_by_default.unwrap_or(base_confirm_by_default),
      default_agent: default_agent.or(base_default_agent),
      pty: pty.unwrap_or_default().merge_over(base_pty),
      agents: merged_agents,
    }
  }
}

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("io: {0}")]
  Io(#[from] std::io::Error),
  #[error("toml: {0}")]
  Toml(#[from] toml::de::Error),
  #[error("unsupported platform: windows is not supported")]
  UnsupportedPlatform,
  #[error("agent `{agent}` is required but not configured")]
  MissingAgentDefinition { agent: String },
  #[error("agent `{agent}` must have at least one start command")]
  InvalidAgentDefinition { agent: String },
}

pub type Result<T> = std::result::Result<T, ConfigError>;

/// Location of the global config file (~/.config/agency/config.toml)
pub fn global_config_path() -> Option<PathBuf> {
  dirs::config_dir().map(|p| p.join("agency").join("config.toml"))
}

/// Location of the project config file (./.agency/config.toml)
pub fn project_config_path(project_root: &Path) -> PathBuf {
  project_root.join(".agency").join("config.toml")
}

/// Write a default project config if it does not exist yet.
pub fn write_default_project_config(project_root: &Path) -> std::io::Result<()> {
  let path = project_config_path(project_root);
  if let Some(parent) = path.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  if !path.exists() {
    let cfg = Config::default();
    let mut s = toml::to_string_pretty(&cfg).unwrap_or_else(|_| String::from(""));
    // Ensure a [pty] section is present with a commented example for detach_keys.
    // We avoid setting a value to keep default None semantics.
    s.push_str(
      "\n# Default agent for `agency new` when --agent is not provided.\n# default_agent = \"opencode\" # or \"claude-code\" or \"fake\"\n\n[pty]\n# Detach key sequence for attach. Comma-separated control keys.\n# Leave unset to use the default Ctrl-Q. Examples:\n# detach_keys = \"ctrl-q\"\n# detach_keys = \"ctrl-p,ctrl-q\"\n",
    );
    std::fs::write(&path, s)?;
  }
  Ok(())
}

/// Load configuration by resolving the default global and project paths.
/// Project config overrides global; both override defaults.
pub fn load(project_root: Option<&Path>) -> Result<Config> {
  let defaults = Config::default();
  let mut cfg = defaults;

  // Global
  if let Some(global_path) = global_config_path()
    && let Ok(s) = fs::read_to_string(&global_path)
  {
    let partial: PartialConfig = toml::from_str(&s)?;
    cfg = partial.merge_over(cfg);
  }

  // Project
  if let Some(root) = project_root {
    let project_path = project_config_path(root);
    if let Ok(s) = fs::read_to_string(&project_path) {
      let partial: PartialConfig = toml::from_str(&s)?;
      cfg = partial.merge_over(cfg);
    }
  }

  validate_agents(&cfg)?;

  Ok(cfg)
}

/// Test helper: load configuration from explicit file paths (if present).
#[cfg(test)]
pub(crate) fn load_from_paths(global: Option<&Path>, project: Option<&Path>) -> Result<Config> {
  let defaults = Config::default();
  let mut cfg = defaults;

  if let Some(g) = global
    && let Ok(s) = fs::read_to_string(g)
  {
    let partial: PartialConfig = toml::from_str(&s)?;
    cfg = partial.merge_over(cfg);
  }

  if let Some(p) = project
    && let Ok(s) = fs::read_to_string(p)
  {
    let partial: PartialConfig = toml::from_str(&s)?;
    cfg = partial.merge_over(cfg);
  }

  validate_agents(&cfg)?;

  Ok(cfg)
}

fn validate_agents(cfg: &Config) -> Result<()> {
  for (name, agent_cfg) in &cfg.agents {
    if agent_cfg.start.is_empty() {
      return Err(ConfigError::InvalidAgentDefinition {
        agent: name.to_string(),
      });
    }
  }

  if let Some(agent) = cfg.default_agent.as_ref() {
    let key = agent_key(agent).to_string();
    if !cfg.agents.contains_key(&key) {
      return Err(ConfigError::MissingAgentDefinition { agent: key });
    }
  }

  Ok(())
}

fn agent_key(agent: &crate::domain::task::Agent) -> &'static str {
  match agent {
    crate::domain::task::Agent::Opencode => "opencode",
    crate::domain::task::Agent::ClaudeCode => "claude-code",
    crate::domain::task::Agent::Fake => "fake",
  }
}

/// Resolve the socket path using AGENCY_SOCKET or platform defaults.
pub fn resolve_socket_path() -> Result<PathBuf> {
  let env_socket = env::var("AGENCY_SOCKET").ok().map(PathBuf::from);
  // Prefer runtime_dir for ephemeral sockets; fall back to data_dir
  let base_dir = runtime_dir().or(data_dir());
  if let Some(val) = env_socket {
    return Ok(val);
  }
  if let Some(dir) = base_dir {
    return Ok(dir.join("agency.sock"));
  }
  Err(ConfigError::UnsupportedPlatform)
}

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
    assert_eq!(fake.start, vec!["sh".to_string(), "-c".to_string(), "echo project".to_string()]);
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
    // Set the environment variable and check resolve_socket_path
    unsafe { std::env::set_var("AGENCY_SOCKET", &p) };
    let got = resolve_socket_path().unwrap();
    assert_eq!(got, p);
    unsafe { std::env::remove_var("AGENCY_SOCKET") };
  }

  #[test]
  fn socket_platform_fallback() {
    // Unset the environment variable to test fallback
    unsafe { std::env::remove_var("AGENCY_SOCKET") };
    let got = resolve_socket_path().unwrap();

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
