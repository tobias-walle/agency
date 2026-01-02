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
use crate::tui::colors::ansi_to_spans;
use crate::utils::sessions::latest_sessions_by_task;
use crate::utils::task::{TaskRef, list_tasks};
use crate::utils::task_columns::{GitMetrics, TaskColumn, TaskRow};

/// Actions that can be triggered from the task table.
#[derive(Clone, Debug)]
pub enum Action {
  None,
  EditOrAttach {
    id: u32,
    session: Option<u64>,
  },
  NewTask {
    start_and_attach: bool,
  },
  StartTask {
    id: u32,
  },
  StopTask {
    id: u32,
  },
  MergeTask {
    id: u32,
  },
  CompleteTask {
    id: u32,
  },
  OpenTask {
    id: u32,
  },
  ShellTask {
    id: u32,
  },
  DeleteTask {
    id: u32,
  },
  ResetTask {
    id: u32,
  },
  /// Open the files overlay for a task.
  OpenFilesOverlay {
    task: TaskRef,
  },
  /// Selection changed, emit focus event with new task id.
  SelectionChanged {
    id: u32,
  },
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
    git_metrics: &HashMap<TaskRef, GitMetrics>,
  ) -> anyhow::Result<()> {
    let mut tasks = list_tasks(&ctx.paths)?;
    tasks.sort_by_key(|t| t.id);

    let latest = latest_sessions_by_task(sessions);

    let rows: Vec<TaskRow> = tasks
      .iter()
      .map(|t| {
        let metrics = git_metrics.get(t).cloned().unwrap_or_default();
        TaskRow::new(ctx, t.clone(), latest.get(t), metrics)
      })
      .collect();

    self.selected = if rows.is_empty() {
      0
    } else {
      self.selected.min(rows.len().saturating_sub(1))
    };
    self.rows = rows;
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

    let visible: HashSet<u32> = self.rows.iter().map(TaskRow::id).collect();
    self.pending_delete.retain(|id, _| visible.contains(id));
  }

  /// Draw the task table.
  pub fn draw(&self, f: &mut ratatui::Frame, area: Rect, focused: bool) {
    let columns = TaskColumn::visible_columns(&self.rows);

    let header_cells: Vec<Cell> = columns.iter().map(|col| Cell::from(col.header())).collect();
    let header = Row::new(header_cells).style(Style::default().fg(Color::Gray));

    let rows = self.rows.iter().map(|r| {
      let pending = self.pending_delete.contains_key(&r.id());
      let cells: Vec<Cell> = columns
        .iter()
        .map(|col| Cell::from(Line::from(ansi_to_spans(&col.cell(r, pending)))))
        .collect();
      Row::new(cells)
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

    let widths: Vec<Constraint> = TaskColumn::width_percentages_for(&columns)
      .into_iter()
      .map(Constraint::Percentage)
      .collect();

    let table = Table::new(rows, widths)
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
      KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
      KeyCode::Down | KeyCode::Char('j') => self.select_next(),
      KeyCode::Enter => self.action_for_selected(|cur| Action::EditOrAttach {
        id: cur.id(),
        session: cur.session_id(),
      }),
      KeyCode::Char('n') => Action::NewTask { start_and_attach: false },
      KeyCode::Char('N') => Action::NewTask { start_and_attach: true },
      KeyCode::Char('s') => self.action_for_id(|id| Action::StartTask { id }),
      KeyCode::Char('S') => self.action_for_id(|id| Action::StopTask { id }),
      KeyCode::Char('m') => self.action_for_id(|id| Action::MergeTask { id }),
      KeyCode::Char('C') => self.action_for_id(|id| Action::CompleteTask { id }),
      KeyCode::Char('o') => self.action_for_id(|id| Action::ShellTask { id }),
      KeyCode::Char('O') => self.action_for_id(|id| Action::OpenTask { id }),
      KeyCode::Char('X') => self.action_for_id(|id| Action::DeleteTask { id }),
      KeyCode::Char('R') => self.action_for_id(|id| Action::ResetTask { id }),
      KeyCode::Char('f') => self.action_for_task(|task| Action::OpenFilesOverlay { task }),
      _ => Action::None,
    }
  }

  fn select_prev(&mut self) -> Action {
    if self.rows.is_empty() {
      return Action::None;
    }
    self.selected = self.selected.saturating_sub(1);
    self.rows.get(self.selected).map_or(Action::None, |sel| {
      Action::SelectionChanged { id: sel.id() }
    })
  }

  fn select_next(&mut self) -> Action {
    if self.rows.is_empty() {
      return Action::None;
    }
    self.selected = (self.selected + 1).min(self.rows.len() - 1);
    self.rows.get(self.selected).map_or(Action::None, |sel| {
      Action::SelectionChanged { id: sel.id() }
    })
  }

  fn action_for_selected<F>(&self, f: F) -> Action
  where
    F: FnOnce(&TaskRow) -> Action,
  {
    self.rows.get(self.selected).map_or(Action::None, f)
  }

  fn action_for_id<F>(&self, f: F) -> Action
  where
    F: FnOnce(u32) -> Action,
  {
    self.rows.get(self.selected).map_or(Action::None, |cur| f(cur.id()))
  }

  fn action_for_task<F>(&self, f: F) -> Action
  where
    F: FnOnce(TaskRef) -> Action,
  {
    self.rows.get(self.selected).map_or(Action::None, |cur| f(cur.task.clone()))
  }

  /// Get the currently selected row, if any.
  pub fn selected_row(&self) -> Option<&TaskRow> {
    self.rows.get(self.selected)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::AgencyPaths;
  use crate::daemon_protocol::TaskMeta;
  use crate::utils::task::TaskRef;
  use crate::utils::term::strip_ansi_control_codes;

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
  fn task_row_with_running_session() {
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: AgencyPaths::new(dir.path(), dir.path()),
      config: crate::config::AgencyConfig::default(),
      tty: crate::utils::tty::Tty::new(),
    };
    let task = make_task(1, "alpha");
    let session = make_session(9, 1, "alpha", "Running", 900);
    let row = TaskRow::new(&ctx, task, Some(&session), GitMetrics::default());

    assert_eq!(row.id(), 1);
    assert_eq!(row.task.slug, "alpha");
    assert_eq!(row.session_id(), Some(9));

    let status_cell = TaskColumn::Status.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&status_cell), "Running");
  }

  #[test]
  fn task_row_without_session_is_draft() {
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: AgencyPaths::new(dir.path(), dir.path()),
      config: crate::config::AgencyConfig::default(),
      tty: crate::utils::tty::Tty::new(),
    };
    let task = make_task(1, "alpha");
    let row = TaskRow::new(&ctx, task, None, GitMetrics::default());

    assert_eq!(row.session_id(), None);

    let status_cell = TaskColumn::Status.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&status_cell), "Draft");
  }

  #[test]
  fn task_row_with_exited_session() {
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: AgencyPaths::new(dir.path(), dir.path()),
      config: crate::config::AgencyConfig::default(),
      tty: crate::utils::tty::Tty::new(),
    };
    let task = make_task(2, "beta");
    let session = make_session(11, 2, "beta", "Exited", 1100);
    let row = TaskRow::new(&ctx, task, Some(&session), GitMetrics::default());

    let status_cell = TaskColumn::Status.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&status_cell), "Exited");
  }

  #[test]
  fn task_row_with_idle_session() {
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = AppContext {
      paths: AgencyPaths::new(dir.path(), dir.path()),
      config: crate::config::AgencyConfig::default(),
      tty: crate::utils::tty::Tty::new(),
    };
    let task = make_task(1, "alpha");
    let session = make_session(10, 1, "alpha", "Idle", 1000);
    let row = TaskRow::new(&ctx, task, Some(&session), GitMetrics::default());

    let status_cell = TaskColumn::Status.cell(&row, false);
    assert_eq!(strip_ansi_control_codes(&status_cell), "Idle");
  }
}
