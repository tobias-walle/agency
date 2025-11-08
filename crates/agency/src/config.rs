use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::command::Command;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use toml::Value as TomlValue;

// Embed repository defaults
const DEFAULT_TOML: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/defaults/agency.toml"));

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentConfig {
  #[serde(default)]
  pub cmd: Vec<String>,
}

impl AgentConfig {
  /// Returns the agent command argv, failing if undefined or empty.
  pub fn get_cmd(&self, name: &str) -> anyhow::Result<Command> {
    if self.cmd.is_empty() {
      anyhow::bail!("{name} not defined or empty")
    }
    Command::new(&self.cmd)
  }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DaemonConfig {
  #[serde(default)]
  pub socket_path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgencyConfig {
  #[serde(default)]
  pub agents: BTreeMap<String, AgentConfig>,
  /// Default agent name when task lacks front matter
  #[serde(default)]
  pub agent: Option<String>,
  #[serde(default)]
  pub daemon: Option<DaemonConfig>,
  #[serde(default)]
  pub keybindings: Option<KeybindingsConfig>,
}

impl AgencyConfig {
  /// Return the agent config for `name` or a helpful error listing known agents.
  pub fn get_agent(&self, name: &str) -> Result<&AgentConfig> {
    if let Some(cfg) = self.agents.get(name) {
      Ok(cfg)
    } else {
      let known: Vec<String> = self.agents.keys().cloned().collect();
      anyhow::bail!("unknown agent: {name}. Known agents: {}", known.join(", "));
    }
  }
}

#[derive(Debug, Clone)]
pub struct AgencyPaths {
  cwd: PathBuf,
}

impl AgencyPaths {
  pub fn new(cwd: impl Into<PathBuf>) -> Self {
    Self { cwd: cwd.into() }
  }

  #[must_use]
  pub fn cwd(&self) -> &PathBuf {
    &self.cwd
  }

  #[must_use]
  pub fn tasks_dir(&self) -> PathBuf {
    self.cwd.join(".agency").join("tasks")
  }

  #[must_use]
  pub fn worktrees_dir(&self) -> PathBuf {
    self.cwd.join(".agency").join("worktrees")
  }
}

#[derive(Debug, Clone)]
pub struct AppContext {
  pub paths: AgencyPaths,
  pub config: AgencyConfig,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct KeybindingsConfig {
  #[serde(default)]
  pub detach: String,
}

fn merge_values(base: &mut TomlValue, overlay: TomlValue) {
  match (base, overlay) {
    (TomlValue::Table(base_tbl), TomlValue::Table(overlay_tbl)) => {
      for (k, v) in overlay_tbl {
        match (base_tbl.get_mut(&k), v) {
          (Some(existing), new_v) => merge_values(existing, new_v),
          (None, new_v) => {
            base_tbl.insert(k, new_v);
          }
        }
      }
    }
    // Arrays and scalars: replace last-wins
    (base_slot, new_v) => {
      *base_slot = new_v;
    }
  }
}

pub fn load_config(cwd: &Path) -> Result<AgencyConfig> {
  // Start with embedded defaults
  let mut merged: TomlValue =
    toml::from_str(DEFAULT_TOML).context("invalid embedded default config")?;

  // Merge global XDG config if present
  let xdg = xdg::BaseDirectories::with_prefix("agency");
  if let Some(global_path) = xdg.find_config_file("agency.toml") {
    let data = fs::read_to_string(&global_path)
      .with_context(|| format!("failed to read {}", global_path.display()))?;
    let val: TomlValue = toml::from_str(&data)
      .with_context(|| format!("invalid TOML in {}", global_path.display()))?;
    merge_values(&mut merged, val);
  }

  // Merge project config if present
  let project_cfg = cwd.join(".agency").join("agency.toml");
  if project_cfg.exists() {
    let data = fs::read_to_string(&project_cfg)
      .with_context(|| format!("failed to read {}", project_cfg.display()))?;
    let val: TomlValue = toml::from_str(&data)
      .with_context(|| format!("invalid TOML in {}", project_cfg.display()))?;
    merge_values(&mut merged, val);
  }

  // Deserialize into strongly typed config
  let merged_str = toml::to_string(&merged).context("failed to serialize merged config")?;
  let cfg: AgencyConfig = toml::from_str(&merged_str).context("failed to parse merged config")?;
  Ok(cfg)
}

/// Compute the daemon socket path based on config and environment.
///
/// Precedence:
/// 1) `config.daemon.socket_path` if set
/// 2) `$XDG_RUNTIME_DIR/agency.sock` if the env var is set
/// 3) Fallback to `~/.local/run/agency.sock`
///
/// Ensures the parent directory exists with 0700 permissions.
#[must_use]
pub fn compute_socket_path(cfg: &AgencyConfig) -> std::path::PathBuf {
  use std::os::unix::fs::PermissionsExt;
  use std::path::PathBuf;

  if let Some(ref daemon) = cfg.daemon
    && let Some(ref p) = daemon.socket_path
  {
    let path = PathBuf::from(p);
    if let Some(dir) = path.parent() {
      let _ = std::fs::create_dir_all(dir);
      let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    return path;
  }

  if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
    let mut path = PathBuf::from(xdg_runtime);
    path.push("agency.sock");
    if let Some(dir) = path.parent() {
      let _ = std::fs::create_dir_all(dir);
      let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    return path;
  }

  // Fallback: ~/.local/run/agency.sock
  let mut path = if let Ok(home) = std::env::var("HOME") {
    PathBuf::from(home)
  } else {
    PathBuf::from(".")
  };
  path.push(".local");
  path.push("run");
  let _ = std::fs::create_dir_all(&path);
  let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
  path.push("agency.sock");
  path
}
