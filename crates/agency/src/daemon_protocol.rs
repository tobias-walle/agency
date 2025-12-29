use anyhow::{Context, Result};
use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct ProjectKey {
  pub repo_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Encode, Decode)]
pub struct TaskMeta {
  pub id: u32,
  pub slug: String,
}

/// Enriched task info for project snapshots
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Encode, Decode)]
pub struct TaskInfo {
  pub id: u32,
  pub slug: String,
  pub base_branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Encode, Decode)]
pub struct SessionInfo {
  pub session_id: u64,
  pub task: TaskMeta,
  pub created_at_ms: u64,
  pub status: String,
  pub clients: u32,
  pub cwd: String,
}

/// Live Git metrics per task
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, Encode, Decode)]
pub struct TaskMetrics {
  pub task: TaskMeta,
  pub uncommitted_add: u64,
  pub uncommitted_del: u64,
  pub commits_ahead: u64,
  pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum C2DControl {
  /// One-shot snapshot of the full project state
  ListProjectState {
    project: ProjectKey,
  },
  SubscribeEvents {
    project: ProjectKey,
  },
  NotifyTasksChanged {
    project: ProjectKey,
  },
  /// TUI lifecycle and follow protocol
  TuiRegister {
    project: ProjectKey,
    pid: u32,
  },
  TuiUnregister {
    project: ProjectKey,
    pid: u32,
  },
  /// Focus changes are delivered to subscribers; CLI should subscribe and then follow
  TuiFocusTaskChange {
    project: ProjectKey,
    tui_id: u32,
    task_id: u32,
  },
  /// Request to follow a specific TUI's focus
  TuiFollow {
    project: ProjectKey,
    tui_id: u32,
  },
  /// List registered TUIs for a project
  TuiList {
    project: ProjectKey,
  },
  /// Request the daemon version string
  GetVersion,
  StopSession {
    session_id: u64,
  },
  StopTask {
    project: ProjectKey,
    task_id: u32,
    slug: String,
  },
  /// Start bootstrap process in the background for a task
  StartBootstrap {
    project: ProjectKey,
    request: BootstrapRequest,
  },
  Shutdown,
  Ping {
    nonce: u64,
  },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum D2CControl {
  /// Streamed or one-shot snapshot of the full project state
  ProjectState {
    project: ProjectKey,
    tasks: Vec<TaskInfo>,
    sessions: Vec<SessionInfo>,
    metrics: Vec<TaskMetrics>,
  },
  /// Reply acknowledging TUI registration with assigned id
  TuiRegistered {
    tui_id: u32,
  },
  /// Reply for follow attempts
  TuiFollowSucceeded {
    tui_id: u32,
  },
  TuiFollowFailed {
    message: String,
  },
  /// Broadcast when a TUI's focused task changes
  TuiFocusTaskChanged {
    project: ProjectKey,
    tui_id: u32,
    task_id: u32,
  },
  /// One-shot list of TUIs for project
  TuiList {
    items: Vec<TuiListItem>,
  },
  Ack {
    stopped: usize,
  },
  Error {
    message: String,
  },
  Goodbye,
  Pong {
    nonce: u64,
  },
  /// Reply with the running daemon version string
  Version {
    version: String,
  },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum C2D {
  Control(C2DControl),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum D2C {
  Control(D2CControl),
}

pub fn write_frame<W: Write, T: Encode>(mut w: W, msg: &T) -> Result<()> {
  let bytes =
    bincode::encode_to_vec(msg, bincode::config::standard()).context("failed to encode frame")?;
  let len: u32 = bytes
    .len()
    .try_into()
    .map_err(|_| anyhow::anyhow!("frame too large"))?;
  w.write_all(&len.to_le_bytes())
    .context("failed to write len")?;
  w.write_all(&bytes).context("failed to write frame bytes")?;
  Ok(())
}

pub fn read_frame<R: Read, T: Decode<()>>(mut r: R) -> Result<T> {
  let mut len_buf = [0_u8; 4];
  r.read_exact(&mut len_buf).context("failed to read len")?;
  let len = u32::from_le_bytes(len_buf) as usize;
  let mut data = vec![0_u8; len];
  r.read_exact(&mut data)
    .context("failed to read frame body")?;
  let (val, _): (T, usize) =
    bincode::decode_from_slice(&data, bincode::config::standard()).context("decode error")?;
  Ok(val)
}

/// Item in the TUI list reply
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct TuiListItem {
  pub tui_id: u32,
  pub pid: u32,
  pub focused_task_id: Option<u32>,
}

/// Request for starting bootstrap in the background.
/// Contains all data needed to run `bootstrap_worktree` and `run_bootstrap_cmd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub struct BootstrapRequest {
  pub repo_root: String,
  pub worktree_dir: String,
  pub task_meta: TaskMeta,
  pub bootstrap_include: Vec<String>,
  pub bootstrap_exclude: Vec<String>,
  pub bootstrap_cmd: Vec<String>,
  pub env_vars: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn encode_decode_tui_messages_roundtrip() {
    let pk = ProjectKey {
      repo_root: "/tmp/repo".to_string(),
    };
    let c = C2D::Control(C2DControl::TuiFollow {
      project: pk.clone(),
      tui_id: 1,
    });
    let bytes = bincode::encode_to_vec(&c, bincode::config::standard()).unwrap();
    let (decoded, _): (C2D, usize) =
      bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(c, decoded);

    let d = D2C::Control(D2CControl::TuiFocusTaskChanged {
      project: pk,
      tui_id: 2,
      task_id: 42,
    });
    let bytes = bincode::encode_to_vec(&d, bincode::config::standard()).unwrap();
    let (decoded, _): (D2C, usize) =
      bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap();
    assert_eq!(d, decoded);
  }
}
