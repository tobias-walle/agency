use serde::{Deserialize, Serialize};

/// Response type for daemon.status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DaemonStatus {
  pub version: String,
  pub pid: u32,
  pub socket_path: String,
}

// ---- Task lifecycle RPC DTOs (Phase 9) ----
use crate::domain::task::{Agent, Status};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskNewParams {
  /// Absolute path to the project root containing the .git and .agency folders
  pub project_root: String,
  /// New task slug, used in filename
  pub slug: String,
  /// Title written into YAML front matter
  pub title: String,
  /// Base branch to branch from, e.g. "main"
  pub base_branch: String,
  /// Optional labels
  #[serde(default)]
  pub labels: Vec<String>,
  /// Agent to use (opencode | claude-code | fake)
  pub agent: Agent,
  /// Optional initial body content
  #[serde(default)]
  pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskInfo {
  pub id: u64,
  pub slug: String,
  pub title: String,
  pub status: Status,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskListParams {
  pub project_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskListResponse {
  pub tasks: Vec<TaskInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskRef {
  #[serde(default)]
  pub id: Option<u64>,
  #[serde(default)]
  pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskStartParams {
  pub project_root: String,
  pub task: TaskRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TaskStartResult {
  pub id: u64,
  pub slug: String,
  pub status: Status,
}

// ---- PTY RPC DTOs (Phase 10) ----

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyAttachParams {
  pub project_root: String,
  pub task: TaskRef,
  pub rows: u16,
  pub cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyAttachResult {
  pub attachment_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyReadParams {
  pub attachment_id: String,
  #[serde(default)]
  pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyReadResult {
  pub data: String,
  pub eof: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyInputParams {
  pub attachment_id: String,
  pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyResizeParams {
  pub attachment_id: String,
  pub rows: u16,
  pub cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PtyDetachParams {
  pub attachment_id: String,
}
