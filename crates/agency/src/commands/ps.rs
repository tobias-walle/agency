use anyhow::Result;
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::pty::protocol::SessionInfo;
use crate::utils::daemon::list_sessions_for_project;
use crate::utils::task::list_tasks;
use crate::utils::term::print_table;

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  tasks.sort_by_key(|t| t.id);

  // Best-effort: query sessions for current project and build latest-session map
  let latest = find_latest_sessions(list_sessions_for_project(ctx));
  let rows: Vec<Vec<String>> = tasks
    .iter()
    .map(|t| {
      let key = (t.id, t.slug.clone());
      if let Some(info) = latest.get(&key) {
        let status_text = get_status_text(Some(info.status.as_str()));
        vec![
          t.id.to_string(),
          t.slug.clone(),
          status_text,
          info.session_id.to_string(),
        ]
      } else {
        let status_text = get_status_text(None);
        vec![t.id.to_string(), t.slug.clone(), status_text, String::new()]
      }
    })
    .collect();

  print_table(&["ID", "SLUG", "STATUS", "SESSION"], &rows);

  Ok(())
}

fn find_latest_sessions(
  sessions: Vec<SessionInfo>,
) -> std::collections::HashMap<(u32, String), SessionInfo> {
  let mut latest: std::collections::HashMap<(u32, String), SessionInfo> =
    std::collections::HashMap::new();
  for session in sessions {
    let key = (session.task.id, session.task.slug.clone());
    match latest.get(&key) {
      None => {
        latest.insert(key, session);
      }
      Some(prev) => {
        if session.created_at_ms >= prev.created_at_ms {
          latest.insert(key, session);
        }
      }
    }
  }
  latest
}

fn get_status_text(status: Option<&str>) -> String {
  match status {
    None => "Draft".yellow().to_string(),
    Some("Exited") => "Exited".red().to_string(),
    Some("Running") => "Running".green().to_string(),
    Some(other) => other.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use regex::Regex;

  fn strip_ansi(input: &str) -> String {
    let re = Regex::new(r"\x1B\[[0-9;]*m").expect("ansi regex");
    re.replace_all(input, "").to_string()
  }

  #[test]
  fn latest_session_selection_by_created_at() {
    let base_a = SessionInfo {
      task: crate::pty::protocol::TaskMeta {
        id: 5,
        slug: "a".to_string(),
      },
      ..Default::default()
    };
    let s1 = SessionInfo {
      session_id: 1,
      created_at_ms: 100,
      ..base_a.clone()
    };
    let s2 = SessionInfo {
      session_id: 2,
      created_at_ms: 200,
      ..base_a.clone()
    };
    let s3_other = SessionInfo {
      session_id: 10,
      task: crate::pty::protocol::TaskMeta {
        id: 6,
        slug: "b".to_string(),
      },
      created_at_ms: 150,
      ..Default::default()
    };
    let map = find_latest_sessions(vec![s1.clone(), s2.clone(), s3_other.clone()]);
    let a_latest = map.get(&(5_u32, "a".to_string())).expect("has a");
    assert_eq!(a_latest.session_id, 2);
    let b_latest = map.get(&(6_u32, "b".to_string())).expect("has b");
    assert_eq!(b_latest.session_id, 10);
  }

  #[test]
  fn status_text_labels_and_colors() {
    let d = get_status_text(None);
    let r = get_status_text(Some("Running"));
    let e = get_status_text(Some("Exited"));
    assert_eq!(strip_ansi(&d), "Draft");
    assert_eq!(strip_ansi(&r), "Running");
    assert_eq!(strip_ansi(&e), "Exited");
    // Ensure ANSI color codes are present in colored output
    assert!(d.contains("\x1B["));
    assert!(r.contains("\x1B["));
    assert!(e.contains("\x1B["));
  }
}
