use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};

use crate::config::AppContext;
use crate::daemon_protocol::SessionInfo;
use crate::utils::task::{TaskFrontmatterExt, TaskRef, list_tasks};

/// Data for a single task row in the table.
#[derive(Clone, Debug)]
pub struct TaskRow {
  pub id: u32,
  pub slug: String,
  pub status: String,
  pub session: Option<u64>,
  pub base: String,
  pub agent: String,
  pub uncommitted_display: String,
  pub commits_display: String,
}

/// Actions that can be triggered from the task table.
#[derive(Clone, Debug)]
pub enum Action {
  None,
  EditOrAttach { id: u32, session: Option<u64> },
  NewTask { start_and_attach: bool },
  StartTask { id: u32 },
  StopTask { id: u32 },
  MergeTask { id: u32 },
  OpenTask { id: u32 },
  ShellTask { id: u32 },
  DeleteTask { id: u32 },
  ResetTask { id: u32 },
  /// Selection changed, emit focus event with new task id.
  SelectionChanged { id: u32 },
}

/// State for the task table component.
pub struct TaskTableState {
  pub rows: Vec<TaskRow>,
  pub selected: usize,
  /// Tasks being deleted to show immediate "Loading" feedback.
  pending_delete: HashMap<u32, Instant>,
  /// TUI id for focus events (set externally).
  pub tui_id: Option<u32>,
}

impl Default for TaskTableState {
  fn default() -> Self {
    Self::new()
  }
}

impl TaskTableState {
  pub fn new() -> Self {
    Self {
      rows: Vec::new(),
      selected: 0,
      pending_delete: HashMap::new(),
      tui_id: None,
    }
  }

  /// Refresh table data from tasks and sessions.
  pub fn refresh(
    &mut self,
    ctx: &AppContext,
    sessions: &[SessionInfo],
    metrics: &[(u32, String, u64, u64, u64)],
  ) -> anyhow::Result<()> {
    let mut tasks = list_tasks(&ctx.paths)?;
    tasks.sort_by_key(|t| t.id);

    let metric_map: HashMap<(u32, String), (u64, u64, u64)> = metrics
      .iter()
      .map(|(id, slug, add, del, ahead)| ((*id, slug.clone()), (*add, *del, *ahead)))
      .collect();

    let (mut rows, sel) = build_rows(ctx, &tasks, sessions, self.selected);

    for r in &mut rows {
      let key = (r.id, r.slug.clone());
      if let Some((a, d, ahead)) = metric_map.get(&key) {
        r.uncommitted_display = if *a == 0 && *d == 0 {
          "+0-0".to_string()
        } else {
          format!("+{a}-{d}")
        };
        r.commits_display = if *ahead == 0 {
          "-".to_string()
        } else {
          ahead.to_string()
        };
      } else {
        r.uncommitted_display = "-".to_string();
        r.commits_display = "-".to_string();
      }
    }

    self.rows = rows;
    self.selected = sel;
    Ok(())
  }

  /// Mark a task as pending delete for immediate UI feedback.
  pub fn mark_pending_delete(&mut self, id: u32) {
    self
      .pending_delete
      .insert(id, Instant::now() + Duration::from_secs(10));
  }

  /// Prune expired pending deletes and remove IDs not in visible rows.
  pub fn prune_pending_deletes(&mut self) {
    let now = Instant::now();
    self.pending_delete.retain(|_, deadline| *deadline > now);

    let visible: HashSet<u32> = self.rows.iter().map(|r| r.id).collect();
    self.pending_delete.retain(|id, _| visible.contains(id));
  }

  /// Draw the task table.
  pub fn draw(&self, f: &mut ratatui::Frame, area: Rect, focused: bool) {
    let header = Row::new([
      Cell::from("ID"),
      Cell::from("SLUG"),
      Cell::from("STATUS"),
      Cell::from("UNCOMMITTED"),
      Cell::from("COMMITS"),
      Cell::from("BASE"),
      Cell::from("AGENT"),
    ])
    .style(Style::default().fg(Color::Gray));

    let rows = self.rows.iter().map(|r| {
      let status_cell = if self.pending_delete.contains_key(&r.id) {
        Cell::from("Loading").style(Style::default().fg(Color::Gray))
      } else {
        Cell::from(r.status.clone()).style(status_style(&r.status))
      };
      Row::new([
        Cell::from(r.id.to_string()),
        Cell::from(r.slug.clone()),
        status_cell,
        uncommitted_cell(&r.uncommitted_display),
        commits_cell(&r.commits_display),
        Cell::from(r.base.clone()),
        Cell::from(r.agent.clone()),
      ])
    });

    let left_title = if focused {
      Line::from("[1] Tasks").fg(Color::Cyan)
    } else {
      Line::from("[1] Tasks")
    };
    let mut table_block = Block::default().borders(Borders::ALL).title(left_title);
    if let Some(id) = self.tui_id {
      let right_title = Line::from(vec![
        Span::raw("TUI ID: "),
        Span::raw(format!("{id}")).fg(Color::Cyan),
      ])
      .right_aligned();
      table_block = table_block.title(right_title);
    }

    let table = Table::new(
      rows,
      [
        Constraint::Percentage(8),
        Constraint::Percentage(20),
        Constraint::Percentage(14),
        Constraint::Percentage(14),
        Constraint::Percentage(10),
        Constraint::Percentage(14),
        Constraint::Percentage(20),
      ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(table_block);

    let mut tstate = TableState::default();
    tstate.select(Some(self.selected));
    f.render_stateful_widget(table, area, &mut tstate);
  }

  /// Handle key events. Returns an Action describing what to do.
  pub fn handle_key(&mut self, key: KeyEvent) -> Action {
    match key.code {
      KeyCode::Up | KeyCode::Char('k') => {
        if !self.rows.is_empty() {
          self.selected = self.selected.saturating_sub(1);
          if let Some(sel) = self.rows.get(self.selected) {
            return Action::SelectionChanged { id: sel.id };
          }
        }
        Action::None
      }
      KeyCode::Down | KeyCode::Char('j') => {
        if !self.rows.is_empty() {
          self.selected = (self.selected + 1).min(self.rows.len() - 1);
          if let Some(sel) = self.rows.get(self.selected) {
            return Action::SelectionChanged { id: sel.id };
          }
        }
        Action::None
      }
      KeyCode::Enter => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::EditOrAttach {
            id: cur.id,
            session: cur.session,
          }
        } else {
          Action::None
        }
      }
      KeyCode::Char('n') => Action::NewTask {
        start_and_attach: false,
      },
      KeyCode::Char('N') => Action::NewTask {
        start_and_attach: true,
      },
      KeyCode::Char('s') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::StartTask { id: cur.id }
        } else {
          Action::None
        }
      }
      KeyCode::Char('S') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::StopTask { id: cur.id }
        } else {
          Action::None
        }
      }
      KeyCode::Char('m') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::MergeTask { id: cur.id }
        } else {
          Action::None
        }
      }
      KeyCode::Char('o') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::OpenTask { id: cur.id }
        } else {
          Action::None
        }
      }
      KeyCode::Char('O') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::ShellTask { id: cur.id }
        } else {
          Action::None
        }
      }
      KeyCode::Char('X') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::DeleteTask { id: cur.id }
        } else {
          Action::None
        }
      }
      KeyCode::Char('R') => {
        if let Some(cur) = self.rows.get(self.selected) {
          Action::ResetTask { id: cur.id }
        } else {
          Action::None
        }
      }
      _ => Action::None,
    }
  }

  /// Get the currently selected row, if any.
  pub fn selected_row(&self) -> Option<&TaskRow> {
    self.rows.get(self.selected)
  }
}

fn build_rows(
  ctx: &AppContext,
  tasks: &[TaskRef],
  sessions: &[SessionInfo],
  prev_selected: usize,
) -> (Vec<TaskRow>, usize) {
  let latest = crate::utils::sessions::latest_sessions_by_task(sessions);

  let mut out = Vec::with_capacity(tasks.len());
  for t in tasks {
    let latest_sess = latest.get(&(t.id, t.slug.clone()));
    let wt_exists = crate::utils::task::worktree_dir(&ctx.paths, t).exists();
    let base_status = crate::utils::status::derive_status(latest_sess, wt_exists);
    let status_str = match if crate::utils::status::is_task_completed(&ctx.paths, t) {
      crate::utils::status::TaskStatus::Completed
    } else {
      base_status
    } {
      crate::utils::status::TaskStatus::Draft => "Draft".to_string(),
      crate::utils::status::TaskStatus::Stopped => "Stopped".to_string(),
      crate::utils::status::TaskStatus::Running => "Running".to_string(),
      crate::utils::status::TaskStatus::Idle => "Idle".to_string(),
      crate::utils::status::TaskStatus::Exited => "Exited".to_string(),
      crate::utils::status::TaskStatus::Completed => "Completed".to_string(),
      crate::utils::status::TaskStatus::Other(s) => s,
    };

    let fm = crate::utils::task::read_task_frontmatter(&ctx.paths, t);
    let agent = crate::utils::task::agent_for_task(&ctx.config, fm.as_ref())
      .unwrap_or_else(|| "-".to_string());
    let base = fm.base_branch(ctx);

    out.push(TaskRow {
      id: t.id,
      slug: t.slug.clone(),
      status: status_str,
      session: latest_sess.map(|s| s.session_id),
      base,
      agent,
      uncommitted_display: "-".to_string(),
      commits_display: "-".to_string(),
    });
  }

  let selected = if out.is_empty() {
    0
  } else {
    prev_selected.min(out.len().saturating_sub(1))
  };
  (out, selected)
}

fn status_style(status: &str) -> Style {
  match status {
    "Running" | "Completed" => Style::default().fg(Color::Green),
    "Idle" => Style::default().fg(Color::Blue),
    "Exited" | "Stopped" => Style::default().fg(Color::Red),
    "Draft" => Style::default().fg(Color::Yellow),
    _ => Style::default(),
  }
}

fn uncommitted_cell(text: &str) -> Cell<'_> {
  if text == "-" {
    return Cell::from(Line::from(vec![
      Span::raw("+0").style(Style::default().fg(Color::Gray)),
      Span::raw("-0").style(Style::default().fg(Color::Gray)),
    ]));
  }
  let s = text.trim();
  if let Some((plus, rest)) = s.strip_prefix('+').and_then(|p| p.split_once('-')) {
    let a_num = plus.parse::<u64>().unwrap_or(0);
    let b_num = rest.parse::<u64>().unwrap_or(0);
    return Cell::from(Line::from(vec![
      if a_num > 0 {
        Span::styled(format!("+{a_num}"), Style::default().fg(Color::Green))
      } else {
        Span::styled("+0", Style::default().fg(Color::Gray))
      },
      if b_num > 0 {
        Span::styled(format!("-{b_num}"), Style::default().fg(Color::Red))
      } else {
        Span::styled("-0", Style::default().fg(Color::Gray))
      },
    ]));
  }
  Cell::from(text.to_string())
}

fn commits_cell(text: &str) -> Cell<'_> {
  if text == "-" || text == "0" {
    return Cell::from(text.to_string()).style(Style::default().fg(Color::Gray));
  }
  Cell::from(text.to_string()).style(Style::default().fg(Color::Cyan))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::daemon_protocol::TaskMeta;

  fn make_task(id: u32, slug: &str) -> TaskRef {
    TaskRef {
      id,
      slug: slug.to_string(),
    }
  }

  fn make_session(
    session_id: u64,
    task_id: u32,
    slug: &str,
    status: &str,
    created_at_ms: u64,
  ) -> SessionInfo {
    SessionInfo {
      session_id,
      task: TaskMeta {
        id: task_id,
        slug: slug.to_string(),
      },
      cwd: "/work".to_string(),
      status: status.to_string(),
      clients: 1,
      created_at_ms,
    }
  }

  #[test]
  fn status_style_mapping() {
    assert_eq!(status_style("Running").fg, Some(Color::Green));
    assert_eq!(status_style("Idle").fg, Some(Color::Blue));
    assert_eq!(status_style("Exited").fg, Some(Color::Red));
    assert_eq!(status_style("Stopped").fg, Some(Color::Red));
    assert_eq!(status_style("Draft").fg, Some(Color::Yellow));
    assert_eq!(status_style("Other").fg, None);
  }

  #[test]
  fn build_rows_session_mapping_and_selection() {
    let tasks = vec![make_task(1, "alpha"), make_task(2, "beta")];
    let sessions = vec![
      make_session(9, 1, "alpha", "Running", 900),
      make_session(10, 2, "beta", "Running", 1000),
      make_session(11, 2, "beta", "Exited", 1100),
    ];
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, sel) = build_rows(&ctx, &tasks, &sessions, 5);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].slug, "alpha");
    assert_eq!(rows[0].status, "Running");
    assert_eq!(rows[0].session, Some(9));
    assert_eq!(rows[1].slug, "beta");
    assert_eq!(rows[1].status, "Exited");
    assert_eq!(rows[1].session, Some(11));
    assert_eq!(sel, 1);
  }

  #[test]
  fn build_rows_show_idle_status() {
    let tasks = vec![make_task(1, "alpha")];
    let sessions = vec![make_session(10, 1, "alpha", "Idle", 1000)];
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, _) = build_rows(&ctx, &tasks, &sessions, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "Idle");
    assert_eq!(rows[0].session, Some(10));
  }

  #[test]
  fn build_rows_without_session_are_draft() {
    let tasks = vec![make_task(1, "alpha")];
    let sessions: Vec<SessionInfo> = Vec::new();
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, _) = build_rows(&ctx, &tasks, &sessions, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "Draft");
    assert_eq!(rows[0].session, None);
  }

  #[test]
  fn selection_zero_when_no_rows() {
    let tasks: Vec<TaskRef> = Vec::new();
    let sessions: Vec<SessionInfo> = Vec::new();
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, sel) = build_rows(&ctx, &tasks, &sessions, 3);
    assert_eq!(rows.len(), 0);
    assert_eq!(sel, 0);
  }
}
