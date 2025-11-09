use owo_colors::OwoColorize as _;

use crate::pty::protocol::SessionInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskStatus {
  Draft,
  Stopped,
  Running,
  Exited,
  Other(String),
}

pub fn derive_status(latest: Option<&SessionInfo>, worktree_exists: bool) -> TaskStatus {
  if let Some(s) = latest {
    return match s.status.as_str() {
      "Running" => TaskStatus::Running,
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
    TaskStatus::Exited => "Exited".red().to_string(),
    TaskStatus::Other(s) => s.clone(),
  }
}
