use std::collections::HashMap;
use std::io::Write as _;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::status::derive_status;
use crate::utils::task::{TaskRef, list_tasks, worktree_dir};
use crate::utils::which;

pub fn run(ctx: &AppContext) -> Result<()> {
  if which::which("fzf").is_none() {
    bail!("fzf is not installed. Install it from https://github.com/junegunn/fzf");
  }

  let mut tasks = list_tasks(&ctx.paths)?;
  if tasks.is_empty() {
    bail!("No tasks found");
  }
  tasks.sort_by_key(|t| t.id);

  let (sessions, wt_exists_map) = get_task_state(ctx, &tasks);
  let latest = latest_sessions_by_task(&sessions);

  let lines: Vec<String> = tasks
    .iter()
    .map(|t| {
      let wt_exists = wt_exists_map.get(t).copied().unwrap_or(false);
      let status = derive_status(latest.get(t), wt_exists);
      format!("{}\t{}\t{}", t.id, t.slug, status.label())
    })
    .collect();

  let input = lines.join("\n");
  let selected = run_fzf(&input)?;

  let Some(selected) = selected else {
    std::process::exit(1);
  };

  let id = selected
    .split('\t')
    .next()
    .and_then(|s| s.parse::<u32>().ok());

  let Some(id) = id else {
    bail!("Failed to parse task ID from selection");
  };

  println!("{id}");
  Ok(())
}

fn get_task_state(
  ctx: &AppContext,
  tasks: &[TaskRef],
) -> (Vec<crate::daemon_protocol::SessionInfo>, HashMap<TaskRef, bool>) {
  let sessions = match get_project_state(ctx) {
    Ok(state) => state.sessions,
    Err(_) => Vec::new(),
  };

  let wt_exists_map: HashMap<TaskRef, bool> = tasks
    .iter()
    .map(|t| (t.clone(), worktree_dir(&ctx.paths, t).exists()))
    .collect();

  (sessions, wt_exists_map)
}

/// Runs fzf with the given input and returns the selected line, or None if cancelled.
///
/// # Errors
/// Returns an error if fzf fails to spawn or encounters an I/O error.
fn run_fzf(input: &str) -> Result<Option<String>> {
  let mut child = Command::new("fzf")
    .args(["--no-multi", "--height=~50%"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::inherit())
    .spawn()?;

  if let Some(stdin) = child.stdin.as_mut() {
    stdin.write_all(input.as_bytes())?;
  }

  let output = child.wait_with_output()?;

  if !output.status.success() {
    return Ok(None);
  }

  let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
  if selected.is_empty() {
    return Ok(None);
  }

  Ok(Some(selected))
}
