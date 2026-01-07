use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::AgencyConfig;
use crate::daemon_protocol::{SessionInfo, TaskMeta};

use super::common::{run_cmd, shell_escape, tmux_args_base};
use super::config::{
  apply_ui_defaults, find_detach_binding, load_default_tmux_config, load_user_tmux_config,
  read_tmux_prefix, tmux_set_env_global, tmux_set_option, tmux_set_option_append,
  tmux_set_option_global, tmux_set_window_option, DetachBinding,
};
use super::pane::pane_dead;
use super::server::ensure_server;

/// Generate a session name for a task.
pub fn session_name(task_id: u32, slug: &str) -> String {
  format!("agency-{task_id}-{slug}")
}

/// Start a new tmux session for a task.
///
/// # Errors
/// Returns an error if any tmux command fails.
pub fn start_session(
  cfg: &AgencyConfig,
  project_root: &Path,
  task: &TaskMeta,
  cwd: &Path,
  program: &str,
  args: &[String],
  env_vars: &HashMap<String, String>,
) -> Result<()> {
  let name = session_name(task.id, &task.slug);
  // Ensure tmux server is up before applying any configuration
  ensure_server(cfg)?;
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
    &format!(",{default_term}:RGB"),
  );
  let _ = tmux_set_option_append(cfg, "", "terminal-features", ",tmux-256color:RGB");
  let _ = tmux_set_option_append(cfg, "", "terminal-features", ",xterm-256color:RGB");
  // Environment for child processes
  let _ = tmux_set_env_global(cfg, "COLORTERM", "truecolor");

  // Create session and launch program
  // Use -e flags to pass env vars to the new session (tmux 3.2+)
  let mut tmux_cmd = std::process::Command::new("tmux");
  tmux_cmd.args(tmux_args_base(cfg)).arg("new-session");
  for (k, v) in env_vars {
    tmux_cmd.arg("-e").arg(format!("{k}={v}"));
  }
  tmux_cmd
    .arg("-d")
    .arg("-s")
    .arg(&name)
    .arg("-c")
    .arg(cwd.display().to_string())
    .arg(program)
    .args(args);
  run_cmd(&mut tmux_cmd).context("tmux new-session failed")?;

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
    DetachBinding::WithPrefix { key } => format!(" Press {prefix}+{key} to detach "),
    DetachBinding::Prefixless { key } => format!(" Press {key} to detach "),
    DetachBinding::None => format!(" Press {prefix}+d to detach "),
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

/// Attach to a tmux session for a task.
///
/// # Errors
/// Returns an error if the tmux command fails.
pub fn attach_session(cfg: &AgencyConfig, task: &TaskMeta) -> Result<()> {
  let name = session_name(task.id, &task.slug);
  // Attach without reapplying config; overrides already sourced on start
  let mut tmux_cmd = attach_cmd(cfg, &name);
  let status = tmux_cmd.status().context("failed to exec tmux attach")?;
  if status.success() {
    Ok(())
  } else {
    anyhow::bail!("tmux attach failed")
  }
}

/// Spawn an attach session process.
///
/// # Errors
/// Returns an error if the process fails to spawn.
pub fn spawn_attach_session(
  cfg: &AgencyConfig,
  task: &TaskMeta,
) -> std::io::Result<std::process::Child> {
  let name = session_name(task.id, &task.slug);
  let mut cmd = attach_cmd(cfg, &name);
  cmd.spawn()
}

/// Prepare a session for attachment by exiting copy-mode if active.
/// This helps prevent scrollback position issues when switching between sessions.
pub fn prepare_session_for_attach(cfg: &AgencyConfig, task: &TaskMeta) {
  let name = session_name(task.id, &task.slug);
  // Exit copy-mode if active (no-op if not in copy-mode)
  // Suppress output since tmux prints "not in a mode" if not in copy-mode
  let _ = std::process::Command::new("tmux")
    .args(tmux_args_base(cfg))
    .arg("send-keys")
    .arg("-t")
    .arg(&name)
    .arg("-X")
    .arg("cancel")
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status();
}

fn attach_cmd(cfg: &AgencyConfig, target_name: &str) -> std::process::Command {
  let mut tmux_cmd = std::process::Command::new("tmux");
  tmux_cmd
    .args(tmux_args_base(cfg))
    .arg("attach-session")
    .arg("-t")
    .arg(target_name);
  tmux_cmd
}

/// Kill a tmux session for a task.
///
/// # Errors
/// Returns an error if the tmux command fails.
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

/// List all tmux sessions for a given project.
///
/// # Errors
/// Returns an error if the tmux command fails.
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
    } else if is_idle(project_root, name) {
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

fn activity_stamp_path(project_root: &Path, session_name: &str) -> PathBuf {
  project_root
    .join(".agency")
    .join("state")
    .join("tmux-activity")
    .join(format!("{session_name}.stamp"))
}

fn is_idle(project_root: &Path, name: &str) -> bool {
  let p = activity_stamp_path(project_root, name);
  let Ok(meta) = std::fs::metadata(&p) else {
    return false;
  };
  let mtime = meta.modified().unwrap_or(std::time::SystemTime::now());
  let age = std::time::SystemTime::now()
    .duration_since(mtime)
    .unwrap_or_default();
  age >= std::time::Duration::from_secs(1)
}
