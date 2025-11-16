use anyhow::Result;

use crate::config::AppContext;
use crate::utils::daemon::get_project_state;
use crate::utils::git::head_branch;
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::status::{TaskStatus, derive_status, is_task_completed, status_label};
use crate::utils::task::{agent_for_task, list_tasks, read_task_frontmatter, worktree_dir};
use crate::utils::term::print_table;
use owo_colors::OwoColorize;

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut tasks = list_tasks(&ctx.paths)?;
  tasks.sort_by_key(|t| t.id);

  // Query project state (sessions + metrics); fallback gracefully when daemon unavailable
  let (sessions, metrics_map) = match get_project_state(ctx) {
    Ok(state) => {
      let m: std::collections::HashMap<(u32, String), (u64, u64, u64)> = state
        .metrics
        .into_iter()
        .map(|m| {
          (
            (m.task.id, m.task.slug),
            (m.uncommitted_add, m.uncommitted_del, m.commits_ahead),
          )
        })
        .collect();
      (state.sessions, m)
    }
    Err(_) => (Vec::new(), std::collections::HashMap::new()),
  };
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
      let (unc_text, commits_text) = if let Some((a, d, ahead)) = metrics_map.get(&key) {
        let plus = if *a == 0 {
          "+0".to_string().dimmed().to_string()
        } else {
          format!("+{a}").green().to_string()
        };
        let minus = if *d == 0 {
          "-0".to_string().dimmed().to_string()
        } else {
          format!("-{d}").red().to_string()
        };
        let unc = format!("{plus}{minus}");
        let commits = if *ahead == 0 {
          "-".to_string().dimmed().to_string()
        } else {
          ahead.to_string().cyan().to_string()
        };
        (unc, commits)
      } else {
        (
          "+0-0".to_string().dimmed().to_string(),
          "-".to_string().dimmed().to_string(),
        )
      };
      vec![
        t.id.to_string(),
        t.slug.clone(),
        status_text,
        unc_text,
        commits_text,
        base.clone(),
        agent.unwrap_or_else(|| "-".to_string()),
      ]
    })
    .collect();

  print_table(
    &[
      "ID",
      "SLUG",
      "STATUS",
      "UNCOMMITTED",
      "COMMITS",
      "BASE",
      "AGENT",
    ],
    &rows,
  );

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
    let draft_text = get_status_text(None);
    let running_text = get_status_text(Some("Running"));
    let exited_text = get_status_text(Some("Exited"));
    let stopped_text = get_status_text(Some("Stopped"));
    let idle_text = get_status_text(Some("Idle"));
    assert_eq!(strip_ansi_control_codes(&draft_text), "Draft");
    assert_eq!(strip_ansi_control_codes(&running_text), "Running");
    assert_eq!(strip_ansi_control_codes(&exited_text), "Exited");
    assert_eq!(strip_ansi_control_codes(&stopped_text), "Stopped");
    assert_eq!(strip_ansi_control_codes(&idle_text), "Idle");
    // Ensure ANSI color codes are present in colored output
    assert!(draft_text.contains("\x1B["));
    assert!(running_text.contains("\x1B["));
    assert!(exited_text.contains("\x1B["));
    assert!(stopped_text.contains("\x1B["));
    assert!(idle_text.contains("\x1B["));
  }
}
