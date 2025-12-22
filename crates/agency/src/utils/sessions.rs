use std::collections::HashMap;

use crate::daemon_protocol::SessionInfo;
use crate::utils::task::TaskRef;

/// Build a map of the latest session per task by `created_at_ms`.
pub fn latest_sessions_by_task(sessions: &[SessionInfo]) -> HashMap<TaskRef, SessionInfo> {
  let mut latest: HashMap<TaskRef, SessionInfo> = HashMap::new();
  for s in sessions {
    let key = TaskRef::from(s.task.clone());
    match latest.get(&key) {
      None => {
        latest.insert(key, s.clone());
      }
      Some(prev) => {
        if s.created_at_ms >= prev.created_at_ms {
          latest.insert(key, s.clone());
        }
      }
    }
  }
  latest
}
