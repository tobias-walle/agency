use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::AgencyConfig;

use super::common::{run_cmd, tmux_args_base};

/// Set a tmux option for a specific target.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_set_option(cfg: &AgencyConfig, target: &str, key: &str, value: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-option")
      .arg("-t")
      .arg(target)
      .arg(key)
      .arg(value),
  )
  .with_context(|| format!("tmux set {key} failed"))
}

/// Append to a tmux option.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_set_option_append(
  cfg: &AgencyConfig,
  target: &str,
  key: &str,
  value: &str,
) -> Result<()> {
  let mut tmux_cmd = std::process::Command::new("tmux");
  tmux_cmd.args(tmux_args_base(cfg)).arg("set-option");
  if target.is_empty() {
    tmux_cmd.arg("-g");
  } else {
    tmux_cmd.arg("-t").arg(target);
  }
  tmux_cmd.arg("-ga").arg(key).arg(value);
  run_cmd(&mut tmux_cmd).with_context(|| format!("tmux append {key} failed"))
}

/// Set a global tmux option.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_set_option_global(cfg: &AgencyConfig, key: &str, value: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-option")
      .arg("-g")
      .arg(key)
      .arg(value),
  )
  .with_context(|| format!("tmux set -g {key} failed"))
}

/// Set a global tmux environment variable.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_set_env_global(cfg: &AgencyConfig, key: &str, value: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-environment")
      .arg("-g")
      .arg(key)
      .arg(value),
  )
}

/// Set a local tmux environment variable for a target session.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_set_env_local(cfg: &AgencyConfig, target: &str, key: &str, value: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-environment")
      .arg("-t")
      .arg(target)
      .arg(key)
      .arg(value),
  )
}

/// Set a tmux window option for a specific target.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_set_window_option(
  cfg: &AgencyConfig,
  target: &str,
  key: &str,
  value: &str,
) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-option")
      .arg("-w")
      .arg("-t")
      .arg(target)
      .arg(key)
      .arg(value),
  )
  .with_context(|| format!("tmux set -w {key} failed"))
}

/// Apply Agency's default UI settings to tmux.
///
/// # Errors
/// Returns an error if any tmux command fails.
pub fn apply_ui_defaults(cfg: &AgencyConfig) -> Result<()> {
  // Enable mouse support by default
  tmux_set_option_global(cfg, "mouse", "on")?;
  // Borders: subtle grey, active cyan accent
  tmux_set_option_global(cfg, "pane-border-style", "fg=colour238")?;
  tmux_set_option_global(cfg, "pane-active-border-style", "fg=colour39")?;

  tmux_set_option_global(cfg, "escape-time", "0")?;

  // Messages and prompts: cyan baseline
  tmux_set_option_global(cfg, "message-style", "fg=colour255,bg=colour24")?;
  tmux_set_option_global(cfg, "message-command-style", "fg=colour255,bg=colour24")?;

  // Mode overlays (copy, choose-tree): cyan baseline
  tmux_set_option_global(cfg, "mode-style", "fg=colour255,bg=colour24")?;

  // Popups (if used): match baseline
  tmux_set_option_global(cfg, "popup-style", "fg=colour255,bg=colour24")?;

  // Display panes overlay colours
  tmux_set_option_global(cfg, "display-panes-colour", "colour238")?;
  tmux_set_option_global(cfg, "display-panes-active-colour", "colour39")?;

  // Optional plugin/user options for scrollbar styling
  tmux_set_option_global(cfg, "@scrollbar", "on")?;
  tmux_set_option_global(cfg, "@scrollbar-style", "fg=colour39,bg=colour24")?;
  Ok(())
}

/// Source a tmux configuration file.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_source_file(cfg: &AgencyConfig, path: &Path) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("source-file")
      .arg(path.display().to_string()),
  )
  .with_context(|| format!("tmux source-file failed for {}", path.display()))
}

/// Load the user's default tmux configuration.
///
/// # Errors
/// Returns an error if sourcing the config file fails.
pub fn load_default_tmux_config(cfg: &AgencyConfig) -> Result<()> {
  // Source user's default tmux config if present.
  if let Ok(home) = std::env::var("HOME") {
    let p1 = PathBuf::from(&home).join(".tmux.conf");
    if p1.exists() {
      tmux_source_file(cfg, &p1)?;
    }
    let p2 = PathBuf::from(&home)
      .join(".config")
      .join("tmux")
      .join("tmux.conf");
    if p2.exists() {
      tmux_source_file(cfg, &p2)?;
    }
  }
  Ok(())
}

/// Load Agency-specific user tmux configuration.
///
/// # Errors
/// Returns an error if sourcing the config file fails.
pub fn load_user_tmux_config(cfg: &AgencyConfig, project_root: Option<&Path>) -> Result<()> {
  // Global XDG config: ~/.config/agency/tmux.conf
  let xdg = xdg::BaseDirectories::with_prefix("agency");
  if let Some(global_tmux) = xdg.find_config_file("tmux.conf")
    && global_tmux.exists()
  {
    tmux_source_file(cfg, &global_tmux)?;
  }
  // Project-local config: <project>/.agency/tmux.conf
  if let Some(root) = project_root {
    let proj_tmux = root.join(".agency").join("tmux.conf");
    if proj_tmux.exists() {
      tmux_source_file(cfg, &proj_tmux)?;
    }
  }
  Ok(())
}

/// Show a global tmux option value.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_show_option_global(cfg: &AgencyConfig, key: &str) -> Result<String> {
  let out = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("show-options")
    .arg("-g")
    .arg("-v")
    .arg(key)
    .output()
    .with_context(|| format!("tmux show-options -g -v {key}"))?;
  if !out.status.success() {
    return Ok(String::new());
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// List tmux key bindings.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn tmux_list_keys(cfg: &AgencyConfig, table: Option<&str>) -> Result<String> {
  let mut cmd = std::process::Command::new("tmux");
  cmd.args(tmux_args_base(cfg)).arg("list-keys");
  if let Some(t) = table {
    cmd.arg("-T").arg(t);
  }
  let out = cmd.output().context("tmux list-keys failed")?;
  if !out.status.success() {
    return Ok(String::new());
  }
  Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetachBinding {
  WithPrefix { key: String },
  Prefixless { key: String },
  None,
}

/// Parse detach-client binding from tmux key listing output.
pub fn parse_detach_binding(prefix_output: &str, global_output: &str) -> DetachBinding {
  // Prefer prefix-table binding
  for line in prefix_output.lines() {
    if !line.contains("detach-client") {
      continue;
    }
    // Expect pattern: bind-key -T prefix [flags...] <key> ... detach-client
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut i = 0usize;
    // find "-T prefix"
    while i + 1 < tokens.len() {
      if tokens[i] == "-T" && tokens[i + 1] == "prefix" {
        i += 2;
        break;
      }
      i += 1;
    }
    if i >= tokens.len() {
      continue;
    }
    // find first non-flag token as key
    while i < tokens.len() && tokens[i].starts_with('-') {
      i += 1;
    }
    if i < tokens.len() {
      let key = tokens[i].to_string();
      return DetachBinding::WithPrefix { key };
    }
  }
  // Fallback: prefixless -n binding
  for line in global_output.lines() {
    if !line.contains("detach-client") {
      continue;
    }
    // Expect: bind-key [flags...] -n <key> ... detach-client
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let mut i = 0usize;
    while i < tokens.len() {
      if tokens[i] == "-n" && i + 1 < tokens.len() {
        let key = tokens[i + 1].to_string();
        return DetachBinding::Prefixless { key };
      }
      i += 1;
    }
  }
  DetachBinding::None
}

/// Read the configured tmux prefix key.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn read_tmux_prefix(cfg: &AgencyConfig) -> Result<String> {
  let p = tmux_show_option_global(cfg, "prefix")?;
  if !p.trim().is_empty() {
    return Ok(p.trim().to_string());
  }
  let p2 = tmux_show_option_global(cfg, "prefix2")?;
  if !p2.trim().is_empty() {
    return Ok(p2.trim().to_string());
  }
  Ok("C-b".to_string())
}

/// Find the detach-client key binding.
pub fn find_detach_binding(cfg: &AgencyConfig) -> DetachBinding {
  let pref = tmux_list_keys(cfg, Some("prefix")).unwrap_or_default();
  let glob = tmux_list_keys(cfg, None).unwrap_or_default();
  parse_detach_binding(&pref, &glob)
}

#[cfg(test)]
mod tests {
  use super::{parse_detach_binding, DetachBinding};

  #[test]
  fn parse_prefix_table_detach() {
    let pref = "bind-key -T prefix d detach-client\n";
    let glob = "";
    let got = parse_detach_binding(pref, glob);
    assert_eq!(got, DetachBinding::WithPrefix { key: "d".into() });
  }

  #[test]
  fn parse_prefix_table_with_flags() {
    let pref = "bind-key -T prefix -r D if-shell -F '#{?client_attached,1,0}' 'detach-client' ''\n";
    let glob = "";
    let got = parse_detach_binding(pref, glob);
    assert_eq!(got, DetachBinding::WithPrefix { key: "D".into() });
  }

  #[test]
  fn parse_prefixless_detach() {
    let pref = "";
    let glob = "bind-key -n M-d detach-client\n";
    let got = parse_detach_binding(pref, glob);
    assert_eq!(got, DetachBinding::Prefixless { key: "M-d".into() });
  }

  #[test]
  fn prefer_prefix_over_prefixless_when_both() {
    let pref = "bind-key -T prefix d detach-client\n";
    let glob = "bind-key -n M-d detach-client\n";
    let got = parse_detach_binding(pref, glob);
    assert_eq!(got, DetachBinding::WithPrefix { key: "d".into() });
  }
}
