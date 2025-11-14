use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::config::AgencyConfig;
use crate::daemon_protocol::{SessionInfo, TaskMeta};

pub fn tmux_socket_path(cfg: &AgencyConfig) -> PathBuf {
  if let Ok(env_path) = std::env::var("AGENCY_TMUX_SOCKET_PATH") {
    return PathBuf::from(env_path);
  }
  if let Some(ref daemon) = cfg.daemon
    && let Some(ref p) = daemon.tmux_socket_path
  {
    return PathBuf::from(p);
  }
  // Default: $XDG_RUNTIME_DIR/agency-tmux.sock or ~/.local/run/agency-tmux.sock
  if let Ok(xdg_runtime) = std::env::var("XDG_RUNTIME_DIR") {
    return PathBuf::from(xdg_runtime).join("agency-tmux.sock");
  }
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  PathBuf::from(home).join(".local/run/agency-tmux.sock")
}

pub fn tmux_args_base(cfg: &AgencyConfig) -> Vec<String> {
  let sock = tmux_socket_path(cfg);
  vec![
    "-S".to_string(),
    sock.display().to_string(),
    "-L".to_string(),
    "agency".to_string(),
  ]
}

pub fn session_name(task_id: u32, slug: &str) -> String {
  format!("agency-{task_id}-{slug}")
}

pub fn start_session(
  cfg: &AgencyConfig,
  project_root: &Path,
  task: &TaskMeta,
  cwd: &Path,
  program: &str,
  args: &[String],
) -> Result<()> {
  let name = session_name(task.id, &task.slug);
  // Resolve default-terminal from the outer environment
  let default_term = std::env::var("TERM")
    .ok()
    .filter(|s| !s.trim().is_empty())
    .unwrap_or_else(|| "tmux-256color".to_string());
  // Load user's default tmux config first, then apply Agency defaults and
  // finally Agency-specific user overrides. Do this BEFORE creating the session,
  // so the initial pane inherits the final environment and options.
  load_default_tmux_config(cfg)?;
  apply_ui_defaults(cfg)?;
  load_user_tmux_config(cfg, Some(project_root))?;
  // Truecolor: set globally so first pane sees it
  tmux_set_option_global(cfg, "default-terminal", &default_term)?;
  // Broadly enable truecolor for terminals; tmux will ignore unsupported
  tmux_set_option_append(cfg, "", "terminal-overrides", ",*:Tc")?;
  let _ = tmux_set_option_append(
    cfg,
    "",
    "terminal-features",
    &format!(",{}:RGB", default_term),
  );
  let _ = tmux_set_option_append(cfg, "", "terminal-features", ",tmux-256color:RGB");
  let _ = tmux_set_option_append(cfg, "", "terminal-features", ",xterm-256color:RGB");
  // Environment for child processes
  let _ = tmux_set_env_global(cfg, "COLORTERM", "truecolor");

  // Create session and launch program
  let mut cmd = std::process::Command::new("tmux");
  cmd
    .args(tmux_args_base(cfg))
    .arg("new-session")
    .arg("-d")
    .arg("-s")
    .arg(&name)
    .arg("-c")
    .arg(cwd.display().to_string())
    .arg(program)
    .args(args);
  run_cmd(&mut cmd).context("tmux new-session failed")?;

  // Auto-close the session when the agent exits
  tmux_set_option(cfg, &name, "remain-on-exit", "off")?;

  // Enable a minimal, theme-friendly status bar
  tmux_set_option(cfg, &name, "status", "on")?;
  tmux_set_option(cfg, &name, "status-style", "bg=default,fg=cyan")?;
  tmux_set_option(cfg, &name, "status-left", " Agency ")?;
  tmux_set_option(cfg, &name, "status-justify", "centre")?;
  // Hide non-current window titles and use the current one for centered task text
  tmux_set_window_option(cfg, &name, "window-status-format", " ")?;
  tmux_set_window_option(
    cfg,
    &name,
    "window-status-current-format",
    &format!(" Task {} (Id: {}) ", task.slug, task.id),
  )?;
  // After sourcing configs, compute the actual detach binding and prefix
  let prefix = read_tmux_prefix(cfg).unwrap_or_else(|_| "C-b".to_string());
  let right = match find_detach_binding(cfg) {
    Ok(DetachBinding::WithPrefix { key }) => format!(" Press {}+{} to detach ", prefix, key),
    Ok(DetachBinding::Prefixless { key }) => format!(" Press {} to detach ", key),
    _ => format!(" Press {}+d to detach ", prefix),
  };
  tmux_set_option(cfg, &name, "status-right", &right)?;

  // Store project root for filtering
  tmux_set_option(
    cfg,
    &name,
    "@agency_root",
    &project_root.display().to_string(),
  )?;

  // Enable pipe-pane to activity stamp
  let stamp = activity_stamp_path(project_root, &name);
  if let Some(parent) = stamp.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  let pipe_cmd = format!("sh -c 'cat > {}'", shell_escape(&stamp));
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("pipe-pane")
      .arg("-o")
      .arg("-t")
      .arg(&name)
      .arg(pipe_cmd),
  )
  .context("tmux pipe-pane failed")?;
  Ok(())
}

pub fn attach_session(cfg: &AgencyConfig, task: &TaskMeta) -> Result<()> {
  let name = session_name(task.id, &task.slug);
  // Attach without reapplying config; overrides already sourced on start
  let mut cmd = std::process::Command::new("tmux");
  cmd
    .args(tmux_args_base(cfg))
    .arg("attach-session")
    .arg("-t")
    .arg(&name);
  let status = cmd.status().context("failed to exec tmux attach")?;
  if status.success() {
    Ok(())
  } else {
    anyhow::bail!("tmux attach failed")
  }
}

pub fn kill_session(cfg: &AgencyConfig, task: &TaskMeta) -> Result<()> {
  let name = session_name(task.id, &task.slug);
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("kill-session")
      .arg("-t")
      .arg(&name),
  )
}

fn run_cmd(cmd: &mut std::process::Command) -> Result<()> {
  let status = cmd.status().with_context(|| format!("spawn {cmd:?}"))?;
  if status.success() {
    Ok(())
  } else {
    anyhow::bail!("command failed: {cmd:?}")
  }
}

fn tmux_set_option(cfg: &AgencyConfig, target: &str, key: &str, value: &str) -> Result<()> {
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

fn tmux_set_option_append(cfg: &AgencyConfig, target: &str, key: &str, value: &str) -> Result<()> {
  let mut cmd = std::process::Command::new("tmux");
  cmd.args(tmux_args_base(cfg)).arg("set-option");
  if target.is_empty() {
    cmd.arg("-g");
  } else {
    cmd.arg("-t").arg(target);
  }
  cmd.arg("-ga").arg(key).arg(value);
  run_cmd(&mut cmd).with_context(|| format!("tmux append {key} failed"))
}

fn tmux_set_option_global(cfg: &AgencyConfig, key: &str, value: &str) -> Result<()> {
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

fn tmux_set_env(cfg: &AgencyConfig, target: &str, key: &str, value: &str) -> Result<()> {
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

fn tmux_set_env_global(cfg: &AgencyConfig, key: &str, value: &str) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-environment")
      .arg("-g")
      .arg(key)
      .arg(value),
  )
}

fn tmux_set_window_option(cfg: &AgencyConfig, target: &str, key: &str, value: &str) -> Result<()> {
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

fn apply_ui_defaults(cfg: &AgencyConfig) -> Result<()> {
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

fn tmux_source_file(cfg: &AgencyConfig, path: &Path) -> Result<()> {
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("source-file")
      .arg(path.display().to_string()),
  )
  .with_context(|| format!("tmux source-file failed for {}", path.display()))
}

fn load_default_tmux_config(cfg: &AgencyConfig) -> Result<()> {
  // Best-effort: quietly source user's default tmux config if present.
  if let Ok(home) = std::env::var("HOME") {
    let p1 = PathBuf::from(&home).join(".tmux.conf");
    if p1.exists() {
      let _ = tmux_source_file(cfg, &p1);
    }
    let p2 = PathBuf::from(&home)
      .join(".config")
      .join("tmux")
      .join("tmux.conf");
    if p2.exists() {
      let _ = tmux_source_file(cfg, &p2);
    }
  }
  Ok(())
}

fn load_user_tmux_config(cfg: &AgencyConfig, project_root: Option<&Path>) -> Result<()> {
  // Global XDG config: ~/.config/agency/tmux.conf
  let xdg = xdg::BaseDirectories::with_prefix("agency");
  if let Some(global_tmux) = xdg.find_config_file("tmux.conf") {
    if global_tmux.exists() {
      let _ = tmux_source_file(cfg, &global_tmux);
    }
  }
  // Project-local config: <project>/.agency/tmux.conf
  if let Some(root) = project_root {
    let proj_tmux = root.join(".agency").join("tmux.conf");
    if proj_tmux.exists() {
      let _ = tmux_source_file(cfg, &proj_tmux);
    }
  }
  Ok(())
}

// Intentionally no one-time flag; Agency starts tmux without config, sets defaults,
// then sources user config so overrides take effect immediately.

fn shell_escape(path: &Path) -> String {
  path
    .display()
    .to_string()
    .replace('\\', "\\\\")
    .replace('"', "\\\"")
    .replace('\'', "'\\''")
}

fn tmux_show_option_global(cfg: &AgencyConfig, key: &str) -> Result<String> {
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

fn tmux_list_keys(cfg: &AgencyConfig, table: Option<&str>) -> Result<String> {
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
enum DetachBinding {
  WithPrefix { key: String },
  Prefixless { key: String },
  None,
}

fn parse_detach_binding(prefix_output: &str, global_output: &str) -> DetachBinding {
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

fn read_tmux_prefix(cfg: &AgencyConfig) -> Result<String> {
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

fn find_detach_binding(cfg: &AgencyConfig) -> Result<DetachBinding> {
  let pref = tmux_list_keys(cfg, Some("prefix")).unwrap_or_default();
  let glob = tmux_list_keys(cfg, None).unwrap_or_default();
  Ok(parse_detach_binding(&pref, &glob))
}

pub fn list_sessions_for_project(
  cfg: &AgencyConfig,
  project_root: &Path,
) -> Result<Vec<SessionInfo>> {
  let output = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("list-sessions")
    .arg("-F")
    .arg("#{session_name}\t#{session_id}\t#{session_created}\t#{@agency_root}\t#{session_attached}")
    .output();
  let output = match output {
    Ok(o) => o,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(e) => return Err(e).context("tmux list-sessions failed"),
  };
  if !output.status.success() {
    return Ok(Vec::new());
  }
  let lines = String::from_utf8_lossy(&output.stdout);
  let mut out = Vec::new();
  for ln in lines.lines() {
    let parts: Vec<&str> = ln.split('\t').collect();
    if parts.len() < 5 {
      continue;
    }
    let name = parts[0];
    let sid_txt = parts[1].trim_start_matches('$');
    let created_txt = parts[2];
    let root = parts[3];
    let clients_txt = parts[4];
    if root != project_root.display().to_string() {
      continue;
    }
    let Some((id, slug)) = parse_session_name(name) else {
      continue;
    };
    let session_id: u64 = sid_txt.parse().unwrap_or(0);
    let created_at_ms: u64 = created_txt.parse::<u64>().unwrap_or(0) * 1000;
    let clients: u32 = clients_txt.parse().unwrap_or(0);
    let cwd = query_session_var(cfg, name, "#{session_path}")?;
    let dead = pane_dead(cfg, name)?;
    let status = if dead {
      "Exited".to_string()
    } else if is_idle(project_root, name)? {
      "Idle".to_string()
    } else {
      "Running".to_string()
    };
    out.push(SessionInfo {
      session_id,
      task: TaskMeta { id, slug },
      created_at_ms,
      status,
      clients,
      cwd,
    });
  }
  Ok(out)
}

fn parse_session_name(name: &str) -> Option<(u32, String)> {
  let prefix = "agency-";
  if !name.starts_with(prefix) {
    return None;
  }
  let rest = &name[prefix.len()..];
  let (id_part, slug) = rest.split_once('-')?;
  let id: u32 = id_part.parse().ok()?;
  Some((id, slug.to_string()))
}

fn query_session_var(cfg: &AgencyConfig, target: &str, format: &str) -> Result<String> {
  let out = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("display-message")
    .arg("-p")
    .arg("-t")
    .arg(target)
    .arg(format)
    .output()
    .context("tmux display-message failed")?;
  if !out.status.success() {
    return Ok(String::new());
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn pane_dead(cfg: &AgencyConfig, target: &str) -> Result<bool> {
  let out = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("list-panes")
    .arg("-F")
    .arg("#{pane_dead}")
    .arg("-t")
    .arg(target)
    .output()
    .context("tmux list-panes failed")?;
  if !out.status.success() {
    return Ok(false);
  }
  let s = String::from_utf8_lossy(&out.stdout);
  Ok(s.lines().any(|l| l.trim() == "1"))
}

fn activity_stamp_path(project_root: &Path, session_name: &str) -> PathBuf {
  project_root
    .join(".agency")
    .join("state")
    .join("tmux-activity")
    .join(format!("{session_name}.stamp"))
}

fn is_idle(project_root: &Path, name: &str) -> Result<bool> {
  let p = activity_stamp_path(project_root, name);
  let meta = match std::fs::metadata(&p) {
    Ok(m) => m,
    Err(_) => return Ok(false),
  };
  let mtime = meta.modified().unwrap_or(std::time::SystemTime::now());
  let age = std::time::SystemTime::now()
    .duration_since(mtime)
    .unwrap_or_default();
  Ok(age >= std::time::Duration::from_secs(1))
}

#[cfg(test)]
mod tests {
  use super::{DetachBinding, parse_detach_binding};

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
