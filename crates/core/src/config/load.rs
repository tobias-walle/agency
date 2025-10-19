use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::paths::{global_config_path, project_config_path};
use super::types::{AgentConfig, Config, LogLevel, PtyConfig, Result};
use super::validate::validate_agents;

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
