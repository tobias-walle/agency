use serde::{Deserialize, Serialize};

/// Response type for daemon.status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DaemonStatus {
  pub version: String,
  pub pid: u32,
  pub socket_path: String,
}
