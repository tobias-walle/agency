use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::command::Command;
use anyhow::{Context, Result};
use owo_colors::OwoColorize as _;
use serde::Deserialize;
use std::collections::BTreeMap;
use toml::Value as TomlValue;

/// Known top-level config keys.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &["agent", "agents", "daemon", "bootstrap", "shell", "editor"];

/// Known keys within `[daemon]` section.
const KNOWN_DAEMON_KEYS: &[&str] = &["socket_path", "tmux_socket_path"];

/// Known keys within `[bootstrap]` section.
const KNOWN_BOOTSTRAP_KEYS: &[&str] = &["include", "exclude", "cmd"];

/// Known keys within each `[agents.<name>]` section.
const KNOWN_AGENT_KEYS: &[&str] = &["cmd"];

// Embed repository defaults
const DEFAULT_TOML: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/defaults/agency.toml"));

/// Embedded config template with all options commented out for documentation.
const CONFIG_TEMPLATE: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/defaults/agency.template.toml"));

/// Returns the config template with all options commented out.
///
/// Use this when generating new config files to show users available options.
#[must_use]
pub fn config_template() -> &'static str {
  CONFIG_TEMPLATE
}

/// Resolve the global config file path.
///
/// # Errors
/// Returns an error if the XDG config home cannot be resolved.
pub fn global_config_path() -> Result<PathBuf> {
  let xdg = xdg::BaseDirectories::with_prefix("agency");
  let config_home = xdg
    .get_config_home()
    .ok_or_else(|| anyhow::anyhow!("unable to resolve XDG config home"))?;
  Ok(config_home.join("agency.toml"))
}

#[must_use]
pub fn global_config_exists() -> bool {
  let xdg = xdg::BaseDirectories::with_prefix("agency");
  xdg.find_config_file("agency.toml").is_some()
}

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
  #[serde(default)]
  pub tmux_socket_path: Option<String>,
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
  pub bootstrap: Option<BootstrapConfig>,
  /// Command to launch when opening a shell. Defaults to user's shell.
  #[serde(default)]
  pub shell: Option<Vec<String>>,
  /// Preferred editor command argv. Falls back to $EDITOR or `vi` when unset.
  #[serde(default)]
  pub editor: Option<Vec<String>>,
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

  /// Returns merged bootstrap config with defaults and de-duplicated lists.
  #[must_use]
  pub fn bootstrap_config(&self) -> BootstrapConfig {
    let mut cfg = self.bootstrap.clone().unwrap_or_default();
    // Always ensure hard excludes are present
    for name in [".git", ".agency"] {
      if !cfg.exclude.iter().any(|e| e == name) {
        cfg.exclude.push(name.to_string());
      }
    }
    // Dedup while preserving order of first occurrence
    dedup_keep_first(&mut cfg.include);
    dedup_keep_first(&mut cfg.exclude);
    cfg
  }

  /// Resolve the editor argv with precedence: config.editor -> $EDITOR -> [`vi`].
  /// Splits the env var via shell-words to support composite commands.
  #[must_use]
  pub fn editor_argv(&self) -> Vec<String> {
    if let Some(v) = &self.editor
      && !v.is_empty()
    {
      return v.clone();
    }
    if let Some(tokens) = editor_env_argv() {
      return tokens;
    }
    vec!["vi".to_string()]
  }
}

#[must_use]
pub(crate) fn editor_env_argv() -> Option<Vec<String>> {
  let raw = std::env::var("EDITOR").ok()?;
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return None;
  }
  let tokens = shell_words::split(trimmed).ok()?;
  if tokens.is_empty() {
    return None;
  }
  Some(tokens)
}

// Helper to deduplicate string vectors while preserving the first occurrence
fn dedup_keep_first(items: &mut Vec<String>) {
  let mut seen = std::collections::BTreeSet::new();
  items.retain(|s| seen.insert(s.clone()));
}

#[derive(Debug, Clone)]
pub struct AgencyPaths {
  root: PathBuf,
  cwd: PathBuf,
}

impl AgencyPaths {
  pub fn new(root: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
    Self {
      root: root.into(),
      cwd: cwd.into(),
    }
  }

  #[must_use]
  pub fn root(&self) -> &PathBuf {
    &self.root
  }

  #[must_use]
  pub fn cwd(&self) -> &PathBuf {
    &self.cwd
  }

  #[must_use]
  pub fn tasks_dir(&self) -> PathBuf {
    self.root.join(".agency").join("tasks")
  }

  #[must_use]
  pub fn worktrees_dir(&self) -> PathBuf {
    self.root.join(".agency").join("worktrees")
  }

  #[must_use]
  pub fn state_dir(&self) -> PathBuf {
    self.root.join(".agency").join("state")
  }

  #[must_use]
  pub fn files_dir(&self) -> PathBuf {
    self.root.join(".agency").join("files")
  }
}

use crate::utils::tty::Tty;

#[derive(Debug, Clone)]
pub struct AppContext {
  pub paths: AgencyPaths,
  pub config: AgencyConfig,
  pub tty: Tty,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BootstrapConfig {
  #[serde(default)]
  pub include: Vec<String>,
  #[serde(default)]
  pub exclude: Vec<String>,
  /// Command to run in newly created worktrees. Args support the `<root>` placeholder.
  /// Empty means disabled. Defaults come from embedded `defaults/agency.toml`.
  #[serde(default)]
  pub cmd: Vec<String>,
}

fn merge_values(base: &mut TomlValue, overlay: TomlValue, path: &str) {
  match (base, overlay) {
    (TomlValue::Table(base_tbl), TomlValue::Table(overlay_tbl)) => {
      for (k, v) in overlay_tbl {
        match (base_tbl.get_mut(&k), v) {
          (Some(existing), new_v) => {
            let next_path = if path.is_empty() {
              k.clone()
            } else {
              format!("{path}.{k}")
            };
            merge_values(existing, new_v, &next_path);
          }
          (None, new_v) => {
            base_tbl.insert(k, new_v);
          }
        }
      }
    }
    (TomlValue::Array(base_arr), TomlValue::Array(mut overlay_arr))
      if path == "bootstrap.include" || path == "bootstrap.exclude" =>
    {
      base_arr.append(&mut overlay_arr);
      // Dedup string arrays
      let mut seen = std::collections::BTreeSet::new();
      base_arr.retain(|v| match v {
        TomlValue::String(s) => seen.insert(s.clone()),
        _ => true,
      });
    }
    // Arrays and scalars: replace last-wins
    (base_slot, new_v) => *base_slot = new_v,
  }
}

/// Warn about unknown keys in a parsed TOML config file.
///
/// Checks top-level keys and nested sections against known key lists.
/// Unknown keys are logged as warnings to help users catch typos.
fn warn_unknown_keys(val: &TomlValue, file_path: &Path) {
  let TomlValue::Table(table) = val else {
    return;
  };

  for key in table.keys() {
    if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
      eprintln!(
        "{}: unknown config key '{}' in {} (did you mean one of: {}?)",
        "warning".yellow(),
        key,
        file_path.display(),
        KNOWN_TOP_LEVEL_KEYS.join(", ")
      );
    }
  }

  if let Some(TomlValue::Table(daemon)) = table.get("daemon") {
    for key in daemon.keys() {
      if !KNOWN_DAEMON_KEYS.contains(&key.as_str()) {
        eprintln!(
          "{}: unknown config key 'daemon.{}' in {} (known keys: {})",
          "warning".yellow(),
          key,
          file_path.display(),
          KNOWN_DAEMON_KEYS.join(", ")
        );
      }
    }
  }

  if let Some(TomlValue::Table(bootstrap)) = table.get("bootstrap") {
    for key in bootstrap.keys() {
      if !KNOWN_BOOTSTRAP_KEYS.contains(&key.as_str()) {
        eprintln!(
          "{}: unknown config key 'bootstrap.{}' in {} (known keys: {})",
          "warning".yellow(),
          key,
          file_path.display(),
          KNOWN_BOOTSTRAP_KEYS.join(", ")
        );
      }
    }
  }

  if let Some(TomlValue::Table(agents)) = table.get("agents") {
    for (agent_name, agent_val) in agents {
      let TomlValue::Table(agent_table) = agent_val else {
        continue;
      };
      for key in agent_table.keys() {
        if !KNOWN_AGENT_KEYS.contains(&key.as_str()) {
          eprintln!(
            "{}: unknown config key 'agents.{}.{}' in {} (known keys: {})",
            "warning".yellow(),
            agent_name,
            key,
            file_path.display(),
            KNOWN_AGENT_KEYS.join(", ")
          );
        }
      }
    }
  }
}

/// Load and merge configuration from defaults, global, and project files.
///
/// # Errors
/// Returns an error if any of the config files cannot be read or parsed
/// as valid TOML, or if serialization of the merged config fails.
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
    warn_unknown_keys(&val, &global_path);
    merge_values(&mut merged, val, "");
  }

  // Merge project config if present
  let project_cfg = cwd.join(".agency").join("agency.toml");
  if project_cfg.exists() {
    let data = fs::read_to_string(&project_cfg)
      .with_context(|| format!("failed to read {}", project_cfg.display()))?;
    let val: TomlValue = toml::from_str(&data)
      .with_context(|| format!("invalid TOML in {}", project_cfg.display()))?;
    warn_unknown_keys(&val, &project_cfg);
    merge_values(&mut merged, val, "");
  }

  // Deserialize into strongly typed config
  let merged_str = toml::to_string(&merged).context("failed to serialize merged config")?;
  let cfg: AgencyConfig = toml::from_str(&merged_str).context("failed to parse merged config")?;
  Ok(cfg)
}

/// Compute the daemon socket path based on config and environment.
///
/// Precedence:
/// 1) `AGENCY_SOCKET_PATH` environment variable (local development override)
/// 2) `config.daemon.socket_path` if set
/// 3) `$XDG_RUNTIME_DIR/agency.sock` if the env var is set
/// 4) Fallback to `~/.local/run/agency.sock`
///
/// Ensures the parent directory exists with 0700 permissions.
#[must_use]
pub fn compute_socket_path(cfg: &AgencyConfig) -> std::path::PathBuf {
  use std::os::unix::fs::PermissionsExt;
  use std::path::PathBuf;

  // Highest precedence: explicit local override via env var
  if let Ok(env_path) = std::env::var("AGENCY_SOCKET_PATH") {
    let path = PathBuf::from(env_path);
    if let Some(dir) = path.parent() {
      let _ = std::fs::create_dir_all(dir);
      let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    return path;
  }

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

#[cfg(test)]
mod tests {
  use super::*;
  use temp_env::with_vars;

  #[test]
  fn compute_prefers_env_over_config_and_xdg() {
    let env_dir = tempfile::tempdir().expect("temp dir env");
    let xdg_dir = tempfile::tempdir().expect("temp dir xdg");
    let cfg_dir = tempfile::tempdir().expect("temp dir cfg");
    let env_sock = env_dir.path().join("dev.sock");
    let cfg_sock = cfg_dir.path().join("cfg.sock");

    with_vars(
      [
        ("AGENCY_SOCKET_PATH", Some(env_sock.display().to_string())),
        (
          "XDG_RUNTIME_DIR",
          Some(xdg_dir.path().display().to_string()),
        ),
      ],
      || {
        let cfg = AgencyConfig {
          daemon: Some(DaemonConfig {
            socket_path: Some(cfg_sock.display().to_string()),
            tmux_socket_path: None,
          }),
          ..Default::default()
        };
        let path = compute_socket_path(&cfg);
        assert_eq!(path, env_sock);
        assert!(path.parent().unwrap().is_dir());
      },
    );
  }

  #[test]
  fn compute_prefers_config_over_xdg() {
    let xdg_dir = tempfile::tempdir().expect("temp dir xdg");
    let cfg_dir = tempfile::tempdir().expect("temp dir cfg");
    let cfg_sock = cfg_dir.path().join("cfg.sock");

    with_vars(
      [
        ("AGENCY_SOCKET_PATH", None),
        (
          "XDG_RUNTIME_DIR",
          Some(xdg_dir.path().display().to_string()),
        ),
      ],
      || {
        let cfg = AgencyConfig {
          daemon: Some(DaemonConfig {
            socket_path: Some(cfg_sock.display().to_string()),
            tmux_socket_path: None,
          }),
          ..Default::default()
        };
        let path = compute_socket_path(&cfg);
        assert_eq!(path, cfg_sock);
        assert!(path.parent().unwrap().is_dir());
      },
    );
  }

  #[test]
  fn compute_uses_xdg_when_no_env_or_config() {
    let xdg_dir = tempfile::tempdir().expect("temp dir xdg");
    with_vars(
      [
        ("AGENCY_SOCKET_PATH", None),
        (
          "XDG_RUNTIME_DIR",
          Some(xdg_dir.path().display().to_string()),
        ),
      ],
      || {
        let cfg = AgencyConfig::default();
        let path = compute_socket_path(&cfg);
        assert_eq!(path, xdg_dir.path().join("agency.sock"));
        assert!(path.parent().unwrap().is_dir());
      },
    );
  }

  #[test]
  fn compute_fallbacks_to_home_local_run() {
    let home_dir = tempfile::tempdir().expect("temp dir home");
    with_vars(
      [
        ("XDG_RUNTIME_DIR", None),
        ("AGENCY_SOCKET_PATH", None),
        ("HOME", Some(home_dir.path().display().to_string())),
      ],
      || {
        let cfg = AgencyConfig::default();
        let path = compute_socket_path(&cfg);
        assert_eq!(
          path,
          home_dir
            .path()
            .join(".local")
            .join("run")
            .join("agency.sock")
        );
        assert!(path.parent().unwrap().is_dir());
      },
    );
  }

  #[test]
  fn default_config_is_empty() {
    let cfg = AgencyConfig::default();
    assert!(cfg.agents.is_empty());
    assert!(cfg.agent.is_none());
    assert!(cfg.daemon.is_none());
    assert!(cfg.bootstrap.is_none());
    assert!(cfg.shell.is_none());
    assert!(cfg.editor.is_none());
  }

  #[test]
  fn agent_config_default_has_empty_cmd() {
    let agent = AgentConfig::default();
    assert!(agent.cmd.is_empty());
  }

  #[test]
  fn agent_config_get_cmd_succeeds_with_valid_cmd() {
    let agent = AgentConfig {
      cmd: vec!["claude".to_string(), "$AGENCY_TASK".to_string()],
    };
    let result = agent.get_cmd("claude");
    assert!(result.is_ok());
  }

  #[test]
  fn agent_config_get_cmd_fails_with_empty_cmd() {
    let agent = AgentConfig::default();
    let result = agent.get_cmd("test");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not defined or empty"));
  }

  #[test]
  fn get_agent_succeeds_when_agent_exists() {
    let mut cfg = AgencyConfig::default();
    cfg.agents.insert(
      "claude".to_string(),
      AgentConfig {
        cmd: vec!["claude".to_string()],
      },
    );

    let result = cfg.get_agent("claude");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().cmd, vec!["claude"]);
  }

  #[test]
  fn get_agent_fails_when_agent_unknown() {
    let mut cfg = AgencyConfig::default();
    cfg.agents.insert(
      "claude".to_string(),
      AgentConfig {
        cmd: vec!["claude".to_string()],
      },
    );

    let result = cfg.get_agent("unknown");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("unknown agent: unknown"));
    assert!(err_msg.contains("Known agents: claude"));
  }

  #[test]
  fn bootstrap_config_adds_hard_excludes() {
    let mut cfg = AgencyConfig::default();
    cfg.bootstrap = Some(BootstrapConfig {
      include: vec![],
      exclude: vec!["node_modules".to_string()],
      cmd: vec![],
    });

    let result = cfg.bootstrap_config();
    assert!(result.exclude.contains(&".git".to_string()));
    assert!(result.exclude.contains(&".agency".to_string()));
    assert!(result.exclude.contains(&"node_modules".to_string()));
  }

  #[test]
  fn bootstrap_config_deduplicates_includes() {
    let mut cfg = AgencyConfig::default();
    cfg.bootstrap = Some(BootstrapConfig {
      include: vec!["file.txt".to_string(), "file.txt".to_string()],
      exclude: vec![],
      cmd: vec![],
    });

    let result = cfg.bootstrap_config();
    assert_eq!(
      result
        .include
        .iter()
        .filter(|s| *s == "file.txt")
        .count(),
      1
    );
  }

  #[test]
  fn bootstrap_config_deduplicates_excludes() {
    let mut cfg = AgencyConfig::default();
    cfg.bootstrap = Some(BootstrapConfig {
      include: vec![],
      exclude: vec![
        "node_modules".to_string(),
        "node_modules".to_string(),
        ".git".to_string(),
      ],
      cmd: vec![],
    });

    let result = cfg.bootstrap_config();
    assert_eq!(
      result
        .exclude
        .iter()
        .filter(|s| *s == "node_modules")
        .count(),
      1
    );
    assert_eq!(result.exclude.iter().filter(|s| *s == ".git").count(), 1);
  }

  #[test]
  fn bootstrap_config_uses_defaults_when_none() {
    let cfg = AgencyConfig::default();
    let result = cfg.bootstrap_config();
    assert!(result.exclude.contains(&".git".to_string()));
    assert!(result.exclude.contains(&".agency".to_string()));
  }

  #[test]
  fn editor_argv_prefers_config() {
    let cfg = AgencyConfig {
      editor: Some(vec!["emacs".to_string(), "-nw".to_string()]),
      ..Default::default()
    };

    with_vars([("EDITOR", Some("vim"))], || {
      let result = cfg.editor_argv();
      assert_eq!(result, vec!["emacs", "-nw"]);
    });
  }

  #[test]
  fn editor_argv_uses_env_when_config_empty() {
    let cfg = AgencyConfig::default();
    with_vars([("EDITOR", Some("nano -w"))], || {
      let result = cfg.editor_argv();
      assert_eq!(result, vec!["nano", "-w"]);
    });
  }

  #[test]
  fn editor_argv_falls_back_to_vi() {
    let cfg = AgencyConfig::default();
    with_vars([("EDITOR", None::<&str>)], || {
      let result = cfg.editor_argv();
      assert_eq!(result, vec!["vi"]);
    });
  }

  #[test]
  fn editor_argv_ignores_empty_config() {
    let cfg = AgencyConfig {
      editor: Some(vec![]),
      ..Default::default()
    };

    with_vars([("EDITOR", Some("vim"))], || {
      let result = cfg.editor_argv();
      assert_eq!(result, vec!["vim"]);
    });
  }

  #[test]
  fn editor_env_argv_parses_simple_command() {
    with_vars([("EDITOR", Some("vim"))], || {
      let result = editor_env_argv();
      assert_eq!(result, Some(vec!["vim".to_string()]));
    });
  }

  #[test]
  fn editor_env_argv_parses_command_with_args() {
    with_vars([("EDITOR", Some("emacs -nw"))], || {
      let result = editor_env_argv();
      assert_eq!(result, Some(vec!["emacs".to_string(), "-nw".to_string()]));
    });
  }

  #[test]
  fn editor_env_argv_handles_missing_env() {
    with_vars([("EDITOR", None::<&str>)], || {
      let result = editor_env_argv();
      assert_eq!(result, None);
    });
  }

  #[test]
  fn editor_env_argv_handles_empty_env() {
    with_vars([("EDITOR", Some(""))], || {
      let result = editor_env_argv();
      assert_eq!(result, None);
    });
  }

  #[test]
  fn editor_env_argv_handles_whitespace_only() {
    with_vars([("EDITOR", Some("   "))], || {
      let result = editor_env_argv();
      assert_eq!(result, None);
    });
  }

  #[test]
  fn dedup_keep_first_preserves_order() {
    let mut items = vec![
      "a".to_string(),
      "b".to_string(),
      "a".to_string(),
      "c".to_string(),
      "b".to_string(),
    ];
    dedup_keep_first(&mut items);
    assert_eq!(items, vec!["a", "b", "c"]);
  }

  #[test]
  fn dedup_keep_first_handles_empty() {
    let mut items: Vec<String> = vec![];
    dedup_keep_first(&mut items);
    assert!(items.is_empty());
  }

  #[test]
  fn dedup_keep_first_handles_no_duplicates() {
    let mut items = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    dedup_keep_first(&mut items);
    assert_eq!(items, vec!["a", "b", "c"]);
  }

  #[test]
  fn merge_values_replaces_scalar() {
    let mut base = toml::from_str::<TomlValue>("key = \"value1\"").unwrap();
    let overlay = toml::from_str::<TomlValue>("key = \"value2\"").unwrap();
    merge_values(&mut base, overlay, "");

    let result: std::collections::BTreeMap<String, String> =
      toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    assert_eq!(result.get("key").unwrap(), "value2");
  }

  #[test]
  fn merge_values_merges_tables() {
    let mut base =
      toml::from_str::<TomlValue>("a = 1\nb = 2").unwrap();
    let overlay = toml::from_str::<TomlValue>("b = 3\nc = 4").unwrap();
    merge_values(&mut base, overlay, "");

    let result: std::collections::BTreeMap<String, i64> =
      toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    assert_eq!(result.get("a").unwrap(), &1);
    assert_eq!(result.get("b").unwrap(), &3);
    assert_eq!(result.get("c").unwrap(), &4);
  }

  #[test]
  fn merge_values_appends_bootstrap_include_arrays() {
    let mut base =
      toml::from_str::<TomlValue>("[bootstrap]\ninclude = [\"a\", \"b\"]").unwrap();
    let overlay = toml::from_str::<TomlValue>("[bootstrap]\ninclude = [\"c\"]").unwrap();
    merge_values(&mut base, overlay, "");

    let result: AgencyConfig = toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    let bootstrap = result.bootstrap.unwrap();
    assert_eq!(bootstrap.include, vec!["a", "b", "c"]);
  }

  #[test]
  fn merge_values_appends_bootstrap_exclude_arrays() {
    let mut base =
      toml::from_str::<TomlValue>("[bootstrap]\nexclude = [\"x\", \"y\"]").unwrap();
    let overlay = toml::from_str::<TomlValue>("[bootstrap]\nexclude = [\"z\"]").unwrap();
    merge_values(&mut base, overlay, "");

    let result: AgencyConfig = toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    let bootstrap = result.bootstrap.unwrap();
    assert_eq!(bootstrap.exclude, vec!["x", "y", "z"]);
  }

  #[test]
  fn merge_values_deduplicates_bootstrap_arrays() {
    let mut base =
      toml::from_str::<TomlValue>("[bootstrap]\ninclude = [\"a\", \"b\"]").unwrap();
    let overlay = toml::from_str::<TomlValue>("[bootstrap]\ninclude = [\"b\", \"c\"]").unwrap();
    merge_values(&mut base, overlay, "");

    let result: AgencyConfig = toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    let bootstrap = result.bootstrap.unwrap();
    assert_eq!(bootstrap.include, vec!["a", "b", "c"]);
  }

  #[test]
  fn merge_values_replaces_non_bootstrap_arrays() {
    let mut base = toml::from_str::<TomlValue>("[agents.test]\ncmd = [\"a\", \"b\"]").unwrap();
    let overlay = toml::from_str::<TomlValue>("[agents.test]\ncmd = [\"c\"]").unwrap();
    merge_values(&mut base, overlay, "");

    let result: AgencyConfig = toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    assert_eq!(result.agents.get("test").unwrap().cmd, vec!["c"]);
  }

  #[test]
  fn merge_values_deeply_nested_tables() {
    let mut base = toml::from_str::<TomlValue>("[daemon]\nsocket_path = \"/tmp/a\"").unwrap();
    let overlay =
      toml::from_str::<TomlValue>("[daemon]\ntmux_socket_path = \"/tmp/b\"").unwrap();
    merge_values(&mut base, overlay, "");

    let result: AgencyConfig = toml::from_str(&toml::to_string(&base).unwrap()).unwrap();
    let daemon = result.daemon.unwrap();
    assert_eq!(daemon.socket_path.unwrap(), "/tmp/a");
    assert_eq!(daemon.tmux_socket_path.unwrap(), "/tmp/b");
  }

  #[test]
  fn load_config_parses_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let cfg = load_config(temp.path()).unwrap();

    assert_eq!(cfg.agent.as_deref(), Some("claude"));
    assert!(cfg.agents.contains_key("claude"));
    assert!(cfg.agents.contains_key("codex"));
    assert!(cfg.agents.contains_key("gemini"));
    assert!(cfg.agents.contains_key("opencode"));
  }

  #[test]
  fn load_config_merges_project_config() {
    let temp = tempfile::tempdir().unwrap();
    let agency_dir = temp.path().join(".agency");
    std::fs::create_dir(&agency_dir).unwrap();

    let project_config = agency_dir.join("agency.toml");
    std::fs::write(
      &project_config,
      r#"
agent = "custom"

[agents.custom]
cmd = ["custom-agent", "$AGENCY_TASK"]
"#,
    )
    .unwrap();

    let cfg = load_config(temp.path()).unwrap();

    assert_eq!(cfg.agent.as_deref(), Some("custom"));
    assert!(cfg.agents.contains_key("custom"));
    assert!(cfg.agents.contains_key("claude"));
  }

  #[test]
  fn load_config_project_overrides_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let agency_dir = temp.path().join(".agency");
    std::fs::create_dir(&agency_dir).unwrap();

    let project_config = agency_dir.join("agency.toml");
    std::fs::write(
      &project_config,
      r#"
[agents.claude]
cmd = ["custom-claude", "arg"]
"#,
    )
    .unwrap();

    let cfg = load_config(temp.path()).unwrap();

    let claude = cfg.agents.get("claude").unwrap();
    assert_eq!(claude.cmd, vec!["custom-claude", "arg"]);
  }

  #[test]
  fn load_config_bootstrap_arrays_append_and_dedupe() {
    let temp = tempfile::tempdir().unwrap();
    let agency_dir = temp.path().join(".agency");
    std::fs::create_dir(&agency_dir).unwrap();

    let project_config = agency_dir.join("agency.toml");
    std::fs::write(
      &project_config,
      r#"
[bootstrap]
include = ["local.txt"]
exclude = [".git", "build"]
"#,
    )
    .unwrap();

    let cfg = load_config(temp.path()).unwrap();
    let bootstrap = cfg.bootstrap.unwrap();

    assert!(bootstrap.include.contains(&"local.txt".to_string()));
    assert!(bootstrap.exclude.contains(&".git".to_string()));
    assert!(bootstrap.exclude.contains(&".agency".to_string()));
    assert!(bootstrap.exclude.contains(&"build".to_string()));
    assert_eq!(
      bootstrap.exclude.iter().filter(|e| *e == ".git").count(),
      1
    );
  }

  #[test]
  fn daemon_config_default_has_none_values() {
    let daemon = DaemonConfig::default();
    assert!(daemon.socket_path.is_none());
    assert!(daemon.tmux_socket_path.is_none());
  }

  #[test]
  fn bootstrap_config_default_has_empty_vecs() {
    let bootstrap = BootstrapConfig::default();
    assert!(bootstrap.include.is_empty());
    assert!(bootstrap.exclude.is_empty());
    assert!(bootstrap.cmd.is_empty());
  }

  #[test]
  fn agency_paths_accessors() {
    let paths = AgencyPaths::new("/repo", "/repo/subdir");
    assert_eq!(paths.root(), &PathBuf::from("/repo"));
    assert_eq!(paths.cwd(), &PathBuf::from("/repo/subdir"));
    assert_eq!(paths.tasks_dir(), PathBuf::from("/repo/.agency/tasks"));
    assert_eq!(
      paths.worktrees_dir(),
      PathBuf::from("/repo/.agency/worktrees")
    );
    assert_eq!(paths.state_dir(), PathBuf::from("/repo/.agency/state"));
    assert_eq!(paths.files_dir(), PathBuf::from("/repo/.agency/files"));
  }
}
