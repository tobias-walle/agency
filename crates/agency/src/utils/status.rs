use crate::daemon_protocol::SessionInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskStatus {
  Draft,
  Stopped,
  Running,
  Idle,
  Exited,
  Other(String),
}

impl TaskStatus {
  /// Returns the display label for this status.
  #[must_use]
  pub fn label(&self) -> &str {
    match self {
      Self::Draft => "Draft",
      Self::Stopped => "Stopped",
      Self::Running => "Running",
      Self::Idle => "Idle",
      Self::Exited => "Exited",
      Self::Other(s) => s,
    }
  }
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
