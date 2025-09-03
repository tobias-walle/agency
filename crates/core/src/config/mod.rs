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
  /// PTY configuration
  pub pty: PtyConfig,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      log_level: LogLevel::Info,
      idle_timeout_secs: 10,
      dwell_secs: 2,
      concurrency: None,
      confirm_by_default: false,
      pty: PtyConfig::default(),
    }
  }
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
  pub pty: Option<PartialPtyConfig>,
}

impl PartialConfig {
  fn merge_over(self, base: Config) -> Config {
    Config {
      log_level: self.log_level.unwrap_or(base.log_level),
      idle_timeout_secs: self.idle_timeout_secs.unwrap_or(base.idle_timeout_secs),
      dwell_secs: self.dwell_secs.unwrap_or(base.dwell_secs),
      concurrency: self.concurrency.unwrap_or(base.concurrency),
      confirm_by_default: self.confirm_by_default.unwrap_or(base.confirm_by_default),
      pty: self.pty.unwrap_or_default().merge_over(base.pty),
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
      "\n[pty]\n# Detach key sequence for attach. Comma-separated control keys.\n# Leave unset to use the default Ctrl-Q. Examples:\n# detach_keys = \"ctrl-q\"\n# detach_keys = \"ctrl-p,ctrl-q\"\n",
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

  Ok(cfg)
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
    assert_eq!(cfg.pty.detach_keys, None);
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
[pty]
detach_keys = "ctrl-p"
"#,
    )
    .unwrap();

    fs::write(
      &project,
      r#"
log_level = "debug"
dwell_secs = 3
[pty]
detach_keys = "ctrl-q"
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
    // pty precedence
    assert_eq!(cfg.pty.detach_keys.as_deref(), Some("ctrl-q"));
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
