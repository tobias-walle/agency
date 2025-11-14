use owo_colors::OwoColorize as _;

use crate::config::AgencyPaths;
use crate::daemon_protocol::SessionInfo;
use crate::utils::task::TaskRef;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskStatus {
  Draft,
  Stopped,
  Running,
  Idle,
  Exited,
  Completed,
  Other(String),
}

pub fn derive_status(latest: Option<&SessionInfo>, worktree_exists: bool) -> TaskStatus {
  if let Some(s) = latest {
    return match s.status.as_str() {
      "Running" => TaskStatus::Running,
      "Idle" => TaskStatus::Idle,
      "Exited" => TaskStatus::Exited,
      other => TaskStatus::Other(other.to_string()),
    };
  }
  if worktree_exists {
    TaskStatus::Stopped
  } else {
    TaskStatus::Draft
  }
}

pub fn status_label(status: &TaskStatus) -> String {
  match status {
    TaskStatus::Draft => "Draft".yellow().to_string(),
    TaskStatus::Stopped => "Stopped".red().to_string(),
    TaskStatus::Running => "Running".green().to_string(),
    TaskStatus::Idle => "Idle".blue().to_string(),
    TaskStatus::Exited => "Exited".red().to_string(),
    TaskStatus::Completed => "Completed".green().to_string(),
    TaskStatus::Other(s) => s.clone(),
  }
}

/// Internal helper to compute a completion flag path for a task.
fn completed_flag_path(paths: &AgencyPaths, task: &TaskRef) -> PathBuf {
  paths
    .state_dir()
    .join("completed")
    .join(format!("{}-{}", task.id, task.slug))
}

/// Returns true if the task has been marked as completed.
#[must_use]
pub fn is_task_completed(paths: &AgencyPaths, task: &TaskRef) -> bool {
  completed_flag_path(paths, task).exists()
}

/// Mark a task as completed (idempotent).
pub fn mark_task_completed(paths: &AgencyPaths, task: &TaskRef) -> Result<()> {
  let flag = completed_flag_path(paths, task);
  if let Some(parent) = flag.parent() {
    let _ = fs::create_dir_all(parent);
  }
  // Write a minimal marker file; content unused.
  fs::write(&flag, b"completed")?;
  Ok(())
}

/// Clear the completed marker for a task.
pub fn clear_task_completed(paths: &AgencyPaths, task: &TaskRef) -> Result<()> {
  let flag = completed_flag_path(paths, task);
  if flag.exists() {
    let _ = fs::remove_file(flag);
  }
  Ok(())
}
