use std::io::{self, IsTerminal as _};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::config::AppContext;
use crate::config::compute_socket_path;
use crate::log_info;
use crate::pty::protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionInfo, read_frame, write_frame,
};
use crate::utils::daemon::list_sessions_for_project as list_sessions;
use crate::utils::editor::open_path;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::{TaskRef, list_tasks, task_file};
use crossbeam_channel::{Receiver, unbounded};
use std::os::unix::net::UnixStream;

#[derive(Clone, Debug)]
struct TaskRow {
  id: u32,
  slug: String,
  status: String,
  session: Option<u64>,
}

#[derive(Default)]
struct AppState {
  rows: Vec<TaskRow>,
  selected: usize,
  mode: Mode,
  slug_input: String,
}

impl AppState {
  fn refresh(&mut self, ctx: &AppContext) -> Result<()> {
    let mut tasks = list_tasks(&ctx.paths)?;
    tasks.sort_by_key(|t| t.id);
    let sessions = list_sessions(ctx);
    let (rows, sel) = build_task_rows(&tasks, &sessions, self.selected);
    self.rows = rows;
    self.selected = sel;
    Ok(())
  }
}

pub fn run(ctx: &AppContext) -> Result<()> {
  if !io::stdout().is_terminal() {
    log_info!("TUI requires a TTY; try 'agency ps' or a real terminal");
    return Ok(());
  }

  // Terminal init
  enable_raw_mode().context("enable raw mode")?;
  let mut stdout = io::stdout();
  crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)
    .context("enter alternate screen")?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend).context("create terminal")?;

  let res = ui_loop(&mut terminal, ctx);

  // Restore terminal
  let out = terminal.backend_mut();
  crossterm::execute!(out, crossterm::terminal::LeaveAlternateScreen).ok();
  disable_raw_mode().ok();
  res
}

#[allow(clippy::too_many_lines)]
fn ui_loop(
  terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
  ctx: &AppContext,
) -> Result<()> {
  let mut state = AppState::default();
  state.refresh(ctx)?;

  // Subscribe to daemon events
  let events_rx = subscribe_events(ctx)?;

  loop {
    // Draw
    terminal.draw(|f| {
      let rects = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(f.area());

      // Table
      let header = Row::new([
        Cell::from("ID"),
        Cell::from("SLUG"),
        Cell::from("STATUS"),
        Cell::from("SESSION"),
      ])
      .style(Style::default().fg(Color::Gray));

      let rows = state.rows.iter().map(|r| {
        Row::new([
          Cell::from(r.id.to_string()),
          Cell::from(r.slug.clone()),
          Cell::from(r.status.clone()).style(status_style(&r.status)),
          Cell::from(r.session.map(|s| s.to_string()).unwrap_or_default()),
        ])
      });

      let table = Table::new(
        rows,
        [
          Constraint::Length(6),
          Constraint::Percentage(50),
          Constraint::Length(10),
          Constraint::Length(8),
        ],
      )
      .header(header)
      .highlight_style(Style::default().bg(Color::DarkGray))
      .block(Block::default().borders(Borders::ALL).title("Tasks"));

      let mut tstate = ratatui::widgets::TableState::default();
      tstate.select(Some(state.selected));
      f.render_stateful_widget(table, rects[0], &mut tstate);

      // Help bar
      let help = Line::from(
        "Select: ↑/↓ j/k | Edit/Attach: ⏎ | New: n/N | Start: S | Delete: X | Reset: R | Quit: q",
      )
      .fg(Color::Blue);
      f.render_widget(
        ratatui::widgets::Paragraph::new(help).alignment(Alignment::Center),
        rects[1],
      );

      // Input overlay
      if let Mode::InputSlug { start_and_attach } = state.mode {
        let area = centered_rect(rects[0], 50, 3);
        let title = if start_and_attach {
          "New Task + Start"
        } else {
          "New Task"
        };
        let block = Block::default()
          .borders(Borders::ALL)
          .title(title)
          .title_alignment(Alignment::Center);
        let input_area = inner(area);
        let prompt = ratatui::widgets::Paragraph::new(Line::from(state.slug_input.clone()));
        f.render_widget(block, area);
        f.render_widget(prompt, input_area);
        // Place the terminal cursor after the input text (left-aligned)
        let mut cx = input_area.x + u16::try_from(state.slug_input.len()).unwrap_or(0);
        let max_x = input_area.x + input_area.width.saturating_sub(1);
        if cx > max_x {
          cx = max_x;
        }
        f.set_cursor_position((cx, input_area.y));
      }
    })?;

    // Events
    // Handle daemon events
    while let Ok(ev) = events_rx.try_recv() {
      match ev {
        UiEvent::TasksChanged | UiEvent::SessionsChanged => {
          state.refresh(ctx)?;
        }
      }
    }

    if event::poll(Duration::from_millis(150))?
      && let Event::Key(key) = event::read()?
    {
      // Ignore key repeats
      if key.kind == KeyEventKind::Repeat {
        continue;
      }
      match state.mode {
        Mode::List => match key.code {
          KeyCode::Char('q') | KeyCode::Esc => break,
          KeyCode::Up | KeyCode::Char('k') => {
            if !state.rows.is_empty() {
              state.selected = state.selected.saturating_sub(1);
            }
          }
          KeyCode::Down | KeyCode::Char('j') => {
            if !state.rows.is_empty() {
              state.selected = (state.selected + 1).min(state.rows.len() - 1);
            }
          }
          KeyCode::Enter => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              restore_terminal(terminal)?;
              if let Some(sid) = cur.session {
                let _ = crate::commands::attach::run_join_session(ctx, sid);
              } else {
                let tref = TaskRef {
                  id: cur.id,
                  slug: cur.slug.clone(),
                };
                let tf = task_file(&ctx.paths, &tref);
                let _ = open_path(&tf);
              }
              reinit_terminal(terminal)?;
              state.refresh(ctx)?;
            }
          }
          KeyCode::Char('n') => {
            state.mode = Mode::InputSlug {
              start_and_attach: false,
            };
            state.slug_input.clear();
          }
          KeyCode::Char('N') => {
            state.mode = Mode::InputSlug {
              start_and_attach: true,
            };
            state.slug_input.clear();
          }
          KeyCode::Char('S') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              restore_terminal(terminal)?;
              let _ = crate::commands::daemon::start();
              let _ = crate::commands::attach::run_with_task(ctx, &cur.id.to_string());
              reinit_terminal(terminal)?;
              state.refresh(ctx)?;
            }
          }
          KeyCode::Char('X') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              restore_terminal(terminal)?;
              let _ = crate::commands::rm::run(ctx, &cur.id.to_string());
              reinit_terminal(terminal)?;
              state.refresh(ctx)?;
            }
          }
          KeyCode::Char('R') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              restore_terminal(terminal)?;
              let tref = TaskRef {
                id: cur.id,
                slug: cur.slug.clone(),
              };
              let _ = crate::utils::daemon::stop_sessions_of_task(ctx, &tref);
              let repo = open_main_repo(ctx.paths.cwd())?;
              let branch = crate::utils::task::branch_name(&tref);
              let wt_dir = crate::utils::task::worktree_dir(&ctx.paths, &tref);
              let _ = crate::utils::git::prune_worktree_if_exists(&repo, &wt_dir)?;
              let _ = crate::utils::git::delete_branch_if_exists(&repo, &branch)?;
              let _ = crate::utils::daemon::notify_tasks_changed(ctx);
              reinit_terminal(terminal)?;
              state.refresh(ctx)?;
            }
          }
          _ => {}
        },
        Mode::InputSlug { start_and_attach } => match key.code {
          KeyCode::Esc => {
            state.mode = Mode::List;
          }
          KeyCode::Enter => {
            let Ok(slug) = crate::utils::task::normalize_and_validate_slug(&state.slug_input)
            else {
              state.mode = Mode::List;
              continue;
            };
            restore_terminal(terminal)?;
            let created = crate::commands::new::run(ctx, &slug, false, None)?;
            if start_and_attach {
              let _ = crate::commands::daemon::start();
              let _ = crate::commands::attach::run_with_task(ctx, &created.id.to_string());
            }
            let _ = crate::utils::daemon::notify_tasks_changed(ctx);
            reinit_terminal(terminal)?;
            state.mode = Mode::List;
            state.slug_input.clear();
            state.refresh(ctx)?;
          }
          KeyCode::Backspace => {
            state.slug_input.pop();
          }
          KeyCode::Char(c) => {
            state.slug_input.push(c);
          }
          _ => {}
        },
      }
    }
  }

  Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
  let out = terminal.backend_mut();
  crossterm::execute!(out, crossterm::terminal::LeaveAlternateScreen)
    .context("leave alt screen")?;
  disable_raw_mode().context("disable raw mode")?;
  Ok(())
}

fn reinit_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
  enable_raw_mode().context("re-enable raw mode")?;
  let mut stdout = io::stdout();
  crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)
    .context("re-enter alt screen")?;
  *terminal = Terminal::new(CrosstermBackend::new(stdout)).context("recreate terminal")?;
  Ok(())
}

fn status_style(status: &str) -> Style {
  match status {
    "Running" => Style::default().fg(Color::Green),
    "Exited" => Style::default().fg(Color::Red),
    "Draft" => Style::default().fg(Color::Yellow),
    _ => Style::default(),
  }
}

fn build_task_rows(
  tasks: &[TaskRef],
  sessions: &[SessionInfo],
  prev_selected: usize,
) -> (Vec<TaskRow>, usize) {
  let mut latest: std::collections::HashMap<(u32, String), SessionInfo> =
    std::collections::HashMap::new();
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

  let mut out = Vec::with_capacity(tasks.len());
  for t in tasks {
    if let Some(info) = latest.get(&(t.id, t.slug.clone())) {
      out.push(TaskRow {
        id: t.id,
        slug: t.slug.clone(),
        status: info.status.clone(),
        session: Some(info.session_id),
      });
    } else {
      out.push(TaskRow {
        id: t.id,
        slug: t.slug.clone(),
        status: "Draft".to_string(),
        session: None,
      });
    }
  }

  let selected = if out.is_empty() {
    0
  } else {
    prev_selected.min(out.len().saturating_sub(1))
  };
  (out, selected)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn status_style_mapping() {
    assert_eq!(status_style("Running").fg, Some(Color::Green));
    assert_eq!(status_style("Exited").fg, Some(Color::Red));
    assert_eq!(status_style("Draft").fg, Some(Color::Yellow));
    assert_eq!(status_style("Other").fg, None);
  }

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
      project: ProjectKey {
        repo_root: "repo".to_string(),
      },
      task: crate::pty::protocol::TaskMeta {
        id: task_id,
        slug: slug.to_string(),
      },
      cwd: "/work".to_string(),
      status: status.to_string(),
      clients: 1,
      created_at_ms,
      stats: crate::pty::protocol::SessionStatsLite {
        bytes_in: 0,
        bytes_out: 0,
        elapsed_ms: 0,
      },
    }
  }

  #[test]
  fn build_rows_session_mapping_and_selection() {
    let tasks = vec![make_task(1, "alpha"), make_task(2, "beta")];
    let sessions = vec![
      make_session(10, 2, "beta", "Running", 1000),
      make_session(11, 2, "beta", "Exited", 1100),
    ];
    let (rows, sel) = build_task_rows(&tasks, &sessions, 5);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].slug, "alpha");
    assert_eq!(rows[0].status, "Draft");
    assert_eq!(rows[0].session, None);
    assert_eq!(rows[1].slug, "beta");
    assert_eq!(rows[1].status, "Exited");
    assert_eq!(rows[1].session, Some(11));
    assert_eq!(sel, 1);
  }

  #[test]
  fn selection_zero_when_no_rows() {
    let tasks: Vec<TaskRef> = Vec::new();
    let sessions: Vec<SessionInfo> = Vec::new();
    let (rows, sel) = build_task_rows(&tasks, &sessions, 3);
    assert_eq!(rows.len(), 0);
    assert_eq!(sel, 0);
  }
}

/// UI events coming from daemon subscription
enum UiEvent {
  TasksChanged,
  SessionsChanged,
}

fn subscribe_events(ctx: &AppContext) -> Result<Receiver<UiEvent>> {
  let (tx, rx) = unbounded::<UiEvent>();
  let socket = compute_socket_path(&ctx.config);
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  std::thread::Builder::new()
    .name("tui-subscribe".to_string())
    .spawn(move || {
      let Ok(mut stream) = UnixStream::connect(&socket) else {
        return;
      };
      let _ = write_frame(
        &mut stream,
        &C2D::Control(C2DControl::SubscribeEvents { project }),
      );
      loop {
        let msg: Result<D2C> = read_frame(&mut stream);
        match msg {
          Ok(D2C::Control(D2CControl::TasksChanged { .. })) => {
            let _ = tx.send(UiEvent::TasksChanged);
          }
          Ok(D2C::Control(D2CControl::SessionsChanged { .. })) => {
            let _ = tx.send(UiEvent::SessionsChanged);
          }
          Ok(_) => {}
          Err(_) => break,
        }
      }
    })?;
  Ok(rx)
}

fn centered_rect(
  area: ratatui::layout::Rect,
  width_pct: u16,
  height_rows: u16,
) -> ratatui::layout::Rect {
  let w = area.width * width_pct / 100;
  let h = height_rows;
  let x = area.x + (area.width.saturating_sub(w)) / 2;
  let y = area.y + (area.height.saturating_sub(h)) / 2;
  ratatui::layout::Rect {
    x,
    y,
    width: w,
    height: h,
  }
}

fn inner(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
  ratatui::layout::Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height.saturating_sub(2),
  }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Mode {
  #[default]
  List,
  InputSlug {
    start_and_attach: bool,
  },
}
