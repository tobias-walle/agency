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
    "-f".to_string(),
    "/dev/null".to_string(),
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

  // Remain on exit
  tmux_set_option(cfg, &name, "remain-on-exit", "on")?;

  // Hide the status bar for a clean, app-controlled UI
  tmux_set_option(cfg, &name, "status", "off")?;
  // Apply Agency defaults globally, then source user config to override
  apply_ui_defaults(cfg)?;
  load_user_tmux_config(cfg, Some(project_root))?;
  tmux_set_option(cfg, &name, "default-terminal", "tmux-256color")?;
  tmux_set_option_append(cfg, &name, "terminal-overrides", ",*256col*:Tc")?;

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
  run_cmd(
    std::process::Command::new("tmux")
      .args(tmux_args_base(cfg))
      .arg("set-option")
      .arg("-t")
      .arg(target)
      .arg("-ga")
      .arg(key)
      .arg(value),
  )
  .with_context(|| format!("tmux append {key} failed"))
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

fn apply_ui_defaults(cfg: &AgencyConfig) -> Result<()> {
  // Borders: subtle grey, active cyan accent
  tmux_set_option_global(cfg, "pane-border-style", "fg=colour238")?;
  tmux_set_option_global(cfg, "pane-active-border-style", "fg=colour39")?;

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
