use std::collections::HashMap;

use crate::pty::protocol::SessionInfo;

/// Build a map of the latest session per task key `(id, slug)` by `created_at_ms`.
pub fn latest_sessions_by_task(sessions: &[SessionInfo]) -> HashMap<(u32, String), SessionInfo> {
  let mut latest: HashMap<(u32, String), SessionInfo> = HashMap::new();
  for s in sessions {
    let key = (s.task.id, s.task.slug.clone());
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
