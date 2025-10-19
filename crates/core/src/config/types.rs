use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

use super::defaults::builtin_agents;

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
