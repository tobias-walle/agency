use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
  /// Whether to ask confirmation by default for destructive commands
  pub confirm_by_default: bool,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      log_level: LogLevel::Info,
      idle_timeout_secs: 10,
      dwell_secs: 2,
      concurrency: None,
      confirm_by_default: true,
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
}

impl PartialConfig {
  fn merge_over(self, base: Config) -> Config {
    Config {
      log_level: self.log_level.unwrap_or(base.log_level),
      idle_timeout_secs: self.idle_timeout_secs.unwrap_or(base.idle_timeout_secs),
      dwell_secs: self.dwell_secs.unwrap_or(base.dwell_secs),
      concurrency: self.concurrency.unwrap_or(base.concurrency),
      confirm_by_default: self.confirm_by_default.unwrap_or(base.confirm_by_default),
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

/// Location of the global config file (~/.config/orchestra/config.toml)
pub fn global_config_path() -> Option<PathBuf> {
  dirs::config_dir().map(|p| p.join("orchestra").join("config.toml"))
}

/// Location of the project config file (./.orchestra/config.toml)
pub fn project_config_path(project_root: &Path) -> PathBuf {
  project_root.join(".orchestra").join("config.toml")
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

/// Resolve the socket path using ORCHESTRA_SOCKET or platform defaults.
/// - Linux: $XDG_RUNTIME_DIR/orchestra.sock or /var/run/orchestra.sock
/// - macOS: $XDG_RUNTIME_DIR/orchestra.sock or /Library/Application Support/orchestra/orchestra.sock
/// - Windows: unsupported
fn resolve_socket_path_from(
  env_socket: Option<String>,
  xdg_runtime: Option<String>,
) -> Result<PathBuf> {
  if let Some(val) = env_socket
    && !val.is_empty()
  {
    return Ok(PathBuf::from(val));
  }

  if let Some(xdg) = xdg_runtime
    && !xdg.is_empty()
  {
    return Ok(PathBuf::from(xdg).join("orchestra.sock"));
  }

  // Platform-specific fallback
  #[cfg(target_os = "linux")]
  {
    return Ok(PathBuf::from("/var/run/orchestra.sock"));
  }

  #[cfg(target_os = "macos")]
  {
    return Ok(PathBuf::from(
      "/Library/Application Support/orchestra/orchestra.sock",
    ));
  }

  #[cfg(target_os = "windows")]
  {
    return Err(ConfigError::UnsupportedPlatform);
  }

  #[allow(unreachable_code)]
  Err(ConfigError::UnsupportedPlatform)
}

pub fn resolve_socket_path() -> Result<PathBuf> {
  let env_socket = env::var("ORCHESTRA_SOCKET").ok();
  let xdg = env::var("XDG_RUNTIME_DIR").ok();
  resolve_socket_path_from(env_socket, xdg)
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
    assert!(cfg.confirm_by_default);
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
"#,
    )
    .unwrap();

    fs::write(
      &project,
      r#"
log_level = "debug"
dwell_secs = 3
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
  }

  #[test]
  fn socket_env_overrides() {
    let td = tempfile::tempdir().unwrap();
    let p = td.path().join("sock");
    let got = super::resolve_socket_path_from(Some(p.to_string_lossy().to_string()), None).unwrap();
    assert_eq!(got, p);
  }

  #[test]
  fn socket_xdg_fallback() {
    let td = tempfile::tempdir().unwrap();
    let got =
      super::resolve_socket_path_from(None, Some(td.path().to_string_lossy().to_string())).unwrap();
    assert_eq!(got, td.path().join("orchestra.sock"));
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn socket_macos_fallback_path() {
    let got = super::resolve_socket_path_from(None, None).unwrap();
    assert_eq!(
      got,
      PathBuf::from("/Library/Application Support/orchestra/orchestra.sock")
    );
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn socket_linux_fallback_path() {
    let got = super::resolve_socket_path_from(None, None).unwrap();
    assert_eq!(got, PathBuf::from("/var/run/orchestra.sock"));
  }
}
