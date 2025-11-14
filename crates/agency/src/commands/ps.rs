use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::list_sessions_for_project;
use crate::utils::git::head_branch;
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::status::{TaskStatus, derive_status, is_task_completed, status_label};
use crate::utils::task::{agent_for_task, list_tasks, read_task_frontmatter, worktree_dir};
use crate::utils::term::print_table;
#[cfg(test)]
use owo_colors::OwoColorize as _;

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  tasks.sort_by_key(|t| t.id);

  // Query sessions for current project and build latest-session map
  let sessions = list_sessions_for_project(ctx)?;
  let latest = latest_sessions_by_task(&sessions);
  let base = head_branch(ctx);
  let rows: Vec<Vec<String>> = tasks
    .iter()
    .map(|t| {
      let key = (t.id, t.slug.clone());
      let latest_sess = latest.get(&key);
      let wt_exists = worktree_dir(&ctx.paths, t).exists();
      let base_status = derive_status(latest_sess, wt_exists);
      let effective_status = if is_task_completed(&ctx.paths, t) {
        TaskStatus::Completed
      } else {
        base_status
      };
      let status_text = status_label(&effective_status);
      let fm = read_task_frontmatter(&ctx.paths, t);
      let agent = agent_for_task(&ctx.config, fm.as_ref());
      vec![
        t.id.to_string(),
        t.slug.clone(),
        status_text,
        latest_sess
          .map(|s| s.session_id.to_string())
          .unwrap_or_default(),
        base.clone(),
        agent.unwrap_or_else(|| "-".to_string()),
      ]
    })
    .collect();

  print_table(&["ID", "SLUG", "STATUS", "SESSION", "BASE", "AGENT"], &rows);

  Ok(())
}

#[cfg(test)]
fn get_status_text(status: Option<&str>) -> String {
  match status {
    None => "Draft".yellow().to_string(),
    Some("Stopped") => "Stopped".red().to_string(),
    Some("Exited") => "Exited".red().to_string(),
    Some("Running") => "Running".green().to_string(),
    Some("Idle") => "Idle".blue().to_string(),
    Some(other) => other.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::daemon_protocol::SessionInfo;
  use crate::utils::term::strip_ansi_control_codes;

  #[test]
  fn latest_session_selection_by_created_at() {
    use crate::utils::sessions::latest_sessions_by_task;
    let base_a = SessionInfo {
      task: crate::daemon_protocol::TaskMeta {
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
      task: crate::daemon_protocol::TaskMeta {
        id: 6,
        slug: "b".to_string(),
      },
      created_at_ms: 150,
      ..Default::default()
    };
    let map = latest_sessions_by_task(&vec![s1.clone(), s2.clone(), s3_other.clone()]);
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
    let s = get_status_text(Some("Stopped"));
    let i = get_status_text(Some("Idle"));
    assert_eq!(strip_ansi_control_codes(&d), "Draft");
    assert_eq!(strip_ansi_control_codes(&r), "Running");
    assert_eq!(strip_ansi_control_codes(&e), "Exited");
    assert_eq!(strip_ansi_control_codes(&s), "Stopped");
    assert_eq!(strip_ansi_control_codes(&i), "Idle");
    // Ensure ANSI color codes are present in colored output
    assert!(d.contains("\x1B["));
    assert!(r.contains("\x1B["));
    assert!(e.contains("\x1B["));
    assert!(s.contains("\x1B["));
    assert!(i.contains("\x1B["));
  }
}
