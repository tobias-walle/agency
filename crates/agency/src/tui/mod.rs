use std::io::{self, IsTerminal as _};
use std::time::Duration;

use anyhow::Error;
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::prelude::Stylize;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::config::AppContext;
use crate::pty::protocol::{
  C2D, C2DControl, D2C, D2CControl, ProjectKey, SessionInfo, read_frame, write_frame,
};
use crate::utils::daemon::{connect_daemon, list_sessions_for_project};
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::{TaskRef, list_tasks};
use crate::{log_error, log_info};
use crossbeam_channel::{Receiver, unbounded};
mod colors;

use crate::utils::interactive::{InteractiveReq, register_sender as register_interactive_sender};
use crate::utils::log::{LogEvent, clear_log_sink, set_log_sink};

#[derive(Clone, Debug)]
struct TaskRow {
  id: u32,
  slug: String,
  status: String,
  session: Option<u64>,
  base: String,
  agent: String,
}

#[derive(Default)]
struct AppState {
  rows: Vec<TaskRow>,
  selected: usize,
  mode: Mode,
  slug_input: String,
  cmd_log: Vec<LogEvent>,
  paused: bool,
}

impl AppState {
  fn refresh(&mut self, ctx: &AppContext) -> Result<()> {
    let mut tasks = list_tasks(&ctx.paths)?;
    tasks.sort_by_key(|t| t.id);
    let sessions = list_sessions_for_project(ctx)?;
    let (rows, sel) = build_task_rows(ctx, &tasks, &sessions, self.selected);
    self.rows = rows;
    self.selected = sel;
    Ok(())
  }

  fn push_log(&mut self, ev: LogEvent) {
    const MAX_LOG: usize = 200;
    self.cmd_log.push(ev);
    if self.cmd_log.len() > MAX_LOG {
      let overflow = self.cmd_log.len() - MAX_LOG;
      self.cmd_log.drain(0..overflow);
    }
  }
}

pub fn run(ctx: &AppContext) -> Result<()> {
  if !io::stdout().is_terminal() {
    log_info!("TUI requires a TTY; try 'agency ps' or a real terminal");
    return Ok(());
  }

  connect_daemon(ctx)?;

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
  // Soft-reset the view (keep scrollback) after leaving the TUI
  crate::utils::term::restore_terminal_state();
  res
}

#[allow(clippy::too_many_lines)]
fn ui_loop(
  terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
  ctx: &AppContext,
) -> Result<()> {
  let mut state = AppState::default();
  state.refresh(ctx).map_err(|err| {
    log_error!("{}", err);
    err
  })?;

  // Subscribe to daemon events
  let events_rx = subscribe_events(ctx).map_err(|err| {
    log_error!("{}", err);
    err
  })?;

  // Wire log sink for routed CLI log lines
  let (log_tx, log_rx) = unbounded::<LogEvent>();
  set_log_sink(log_tx.clone());

  // Interactive control channel (Begin/End) for just-in-time terminal switching
  let (itx, irx) = unbounded::<InteractiveReq>();
  register_interactive_sender(itx);

  loop {
    // Drain routed logs without blocking
    while let Ok(ev) = log_rx.try_recv() {
      state.push_log(ev);
    }

    // Handle interactive begin/end requests
    while let Ok(req) = irx.try_recv() {
      match req {
        InteractiveReq::Begin { ack } => {
          restore_terminal(terminal)?;
          state.paused = true;
          let _ = ack.send(());
        }
        InteractiveReq::End { ack } => {
          reinit_terminal(terminal)?;
          state.paused = false;
          let _ = ack.send(());
          state.refresh(ctx).map_err(|err| {
            log_error!("{}", err);
            err
          })?;
        }
      }
    }

    // When paused an interactive program owns the terminal. Do not draw or read keys
    // here to avoid stealing input or corrupting its screen. Keep the loop alive to
    // service interactive End acks, but throttle to avoid busy spinning.
    if state.paused {
      std::thread::sleep(Duration::from_millis(50));
      continue;
    }

    // Draw
    terminal.draw(|f| {
      // Build help lines using smart item-boundary wrapping
      let help_items = [
        "Select: ↑/↓ j/k",
        "Edit/Attach: ⏎",
        "New: n/N",
        "Start: s",
        "Stop: S",
        "Merge: m",
        "Open: o",
        "Delete: X",
        "Reset: R",
        "Quit: ^C",
      ];
      let mut help_lines = layout_help_lines(&help_items, f.area().width);
      let help_rows = help_lines.len().try_into().unwrap_or(1_u16).clamp(1, 3);

      let rects = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(5),
        Constraint::Length(help_rows),
      ])
      .split(f.area());

      // Table
      let header = Row::new([
        Cell::from("ID"),
        Cell::from("SLUG"),
        Cell::from("STATUS"),
        Cell::from("SESSION"),
        Cell::from("BASE"),
        Cell::from("AGENT"),
      ])
      .style(Style::default().fg(Color::Gray));

      let rows = state.rows.iter().map(|r| {
        Row::new([
          Cell::from(r.id.to_string()),
          Cell::from(r.slug.clone()),
          Cell::from(r.status.clone()).style(status_style(&r.status)),
          Cell::from(r.session.map(|s| s.to_string()).unwrap_or_default()),
          Cell::from(r.base.clone()),
          Cell::from(r.agent.clone()),
        ])
      });

      let table = Table::new(
        rows,
        [
          Constraint::Length(6),
          Constraint::Percentage(40),
          Constraint::Length(10),
          Constraint::Length(8),
          Constraint::Length(12),
          Constraint::Length(12),
        ],
      )
      .header(header)
      .highlight_style(Style::default().bg(Color::DarkGray))
      .block(Block::default().borders(Borders::ALL).title("Tasks"));

      let mut tstate = ratatui::widgets::TableState::default();
      tstate.select(Some(state.selected));
      f.render_stateful_widget(table, rects[0], &mut tstate);

      // Command Log
      let mut lines: Vec<Line> = Vec::with_capacity(state.cmd_log.len());
      for ev in &state.cmd_log {
        match ev {
          LogEvent::Command(s) => {
            lines.push(Line::from(format!("> {s}")).fg(Color::Gray));
          }
          LogEvent::Line { ansi, .. } => {
            let spans: Vec<Span> = colors::ansi_to_spans(ansi);
            lines.push(Line::from(spans));
          }
        }
      }
      let log_block = Block::default().borders(Borders::ALL).title("Command Log");
      // Render only the visible lines (auto-scroll to latest)
      let content_h = rects[1].height.saturating_sub(2) as usize; // minus borders
      let total_lines = lines.len();
      let start = total_lines.saturating_sub(content_h);
      let visible = lines[start..].to_vec();
      let log_para = Paragraph::new(visible).block(log_block);
      f.render_widget(log_para, rects[1]);

      // Help bar
      // Style lines
      help_lines = help_lines
        .into_iter()
        .map(|ln| ln.fg(Color::Blue))
        .collect();
      f.render_widget(
        ratatui::widgets::Paragraph::new(help_lines).alignment(Alignment::Center),
        rects[2],
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
          state.refresh(ctx).map_err(|err| {
            log_error!("{}", err);
            err
          })?;
        }
        UiEvent::Disconnected(err) => {
          log_error!("{}", err);
          return Err(err);
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
          KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
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
              // Launch interactive action on a background thread; the command will
              // wrap its interactive boundary using utils::interactive::scope
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  if let Some(sid) = cur.session {
                    let _ = crate::commands::attach::run_join_session(&ctx, sid);
                  } else {
                    let tref = TaskRef {
                      id: cur.id,
                      slug: cur.slug.clone(),
                    };
                    let _ = crate::commands::edit::run(&ctx, &tref.id.to_string());
                  }
                }
              });
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
          KeyCode::Char('s') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              // Background start (no attach)
              let id_str = cur.id.to_string();
              state.push_log(LogEvent::Command(format!("agency start {id_str}")));
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  if let Err(err) = crate::commands::start::run(&ctx, &id_str) {
                    crate::log_error!("Start failed: {}", err);
                  }
                }
              });
            }
          }
          KeyCode::Char('S') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              let tref = TaskRef {
                id: cur.id,
                slug: cur.slug.clone(),
              };
              state.push_log(LogEvent::Command(format!("agency stop --task {}", tref.id)));
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  if let Err(err) =
                    crate::commands::stop::run(&ctx, Some(&tref.id.to_string()), None)
                  {
                    crate::log_error!("Stop failed: {}", err);
                  }
                }
              });
            }
          }
          KeyCode::Char('m') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              let id_str = cur.id.to_string();
              state.push_log(LogEvent::Command(format!("agency merge {id_str}")));
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  if let Err(err) = crate::commands::merge::run(&ctx, &id_str, None) {
                    crate::log_error!("Merge failed: {}", err);
                  }
                }
              });
            }
          }
          KeyCode::Char('o') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              // Open worktree on a background thread; open action internally
              // wraps interactive boundary when needed
              std::thread::spawn({
                let ctx = ctx.clone();
                let id = cur.id.to_string();
                move || {
                  let _ = crate::commands::open::run(&ctx, &id);
                }
              });
            }
          }
          KeyCode::Char('X') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              state.push_log(LogEvent::Command(format!("agency rm {}", cur.id)));
              let ident = cur.id.to_string();
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  let _ = crate::commands::rm::run_force(&ctx, &ident);
                }
              });
            }
          }
          KeyCode::Char('R') => {
            if let Some(cur) = state.rows.get(state.selected).cloned() {
              let tref = TaskRef {
                id: cur.id,
                slug: cur.slug.clone(),
              };
              state.push_log(LogEvent::Command(format!("agency reset {}", tref.id)));
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  if let Err(err) = crate::commands::reset::run(&ctx, &tref.id.to_string()) {
                    crate::log_error!("Reset failed: {}", err);
                  }
                }
              });
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
            if start_and_attach {
              // Create and start on a background thread to avoid blocking the TUI
              // (editor for the new task runs within an interactive scope)
              state.push_log(LogEvent::Command(format!("agency new {slug} + start")));
              std::thread::spawn({
                let ctx = ctx.clone();
                let slug = slug.clone();
                move || match crate::commands::new::run(&ctx, &slug, false, None) {
                  Ok(created) => {
                    let id_str = created.id.to_string();
                    if let Err(err) = crate::commands::start::run(&ctx, &id_str) {
                      crate::log_error!("Start failed: {}", err);
                    }
                  }
                  Err(err) => {
                    crate::log_error!("New failed: {}", err);
                  }
                }
              });
            } else {
              // Interactive editor open without attach; run on background thread
              std::thread::spawn({
                let ctx = ctx.clone();
                move || {
                  let _ = crate::commands::new::run(&ctx, &slug, false, None);
                }
              });
            }
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

  clear_log_sink();
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
    "Idle" => Style::default().fg(Color::Blue),
    "Exited" | "Stopped" => Style::default().fg(Color::Red),
    "Draft" => Style::default().fg(Color::Yellow),
    _ => Style::default(),
  }
}

fn build_task_rows(
  ctx: &AppContext,
  tasks: &[TaskRef],
  sessions: &[SessionInfo],
  prev_selected: usize,
) -> (Vec<TaskRow>, usize) {
  let latest = crate::utils::sessions::latest_sessions_by_task(sessions);
  let base = crate::utils::git::head_branch(ctx);

  let mut out = Vec::with_capacity(tasks.len());
  for t in tasks {
    let latest_sess = latest.get(&(t.id, t.slug.clone()));
    let wt_exists = crate::utils::task::worktree_dir(&ctx.paths, t).exists();
    let status = crate::utils::status::derive_status(latest_sess, wt_exists);
    let status_str = match status {
      crate::utils::status::TaskStatus::Draft => "Draft".to_string(),
      crate::utils::status::TaskStatus::Stopped => "Stopped".to_string(),
      crate::utils::status::TaskStatus::Running => "Running".to_string(),
      crate::utils::status::TaskStatus::Idle => "Idle".to_string(),
      crate::utils::status::TaskStatus::Exited => "Exited".to_string(),
      crate::utils::status::TaskStatus::Other(s) => s,
    };

    let fm = crate::utils::task::read_task_frontmatter(&ctx.paths, t);
    let agent = crate::utils::task::agent_for_task(&ctx.config, fm.as_ref())
      .unwrap_or_else(|| "-".to_string());

    out.push(TaskRow {
      id: t.id,
      slug: t.slug.clone(),
      status: status_str,
      session: latest_sess.map(|s| s.session_id),
      base: base.clone(),
      agent,
    });
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
  fn help_layout_item_boundary_wrap() {
    let items = [
      "Select: ↑/↓ j/k",
      "Edit/Attach: ⏎",
      "New: n/N",
      "Start: s",
      "Stop: S",
      "Merge: m",
      "Open: o",
      "Delete: X",
      "Reset: R",
      "Quit: ^C",
    ];
    // Very narrow should result in many lines but keep pairs intact
    let lines = layout_help_lines(&items, 20);
    assert!(lines.len() >= 2);

    // Ensure the last line contains Reset and Quit together when width allows
    let lines2 = layout_help_lines(&items, 60);
    let all_line_texts: Vec<String> = lines2
      .iter()
      .map(|ln| ln.spans.iter().map(|s| s.content.to_string()).collect())
      .collect();
    assert!(all_line_texts.iter().any(|t| t.contains("Reset: R")));
    assert!(all_line_texts.iter().any(|t| t.contains("Quit: ^C")));
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
      make_session(9, 1, "alpha", "Running", 900),
      make_session(10, 2, "beta", "Running", 1000),
      make_session(11, 2, "beta", "Exited", 1100),
    ];
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = crate::config::AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, sel) = build_task_rows(&ctx, &tasks, &sessions, 5);
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
    let ctx = crate::config::AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, _) = build_task_rows(&ctx, &tasks, &sessions, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "Idle");
    assert_eq!(rows[0].session, Some(10));
  }

  #[test]
  fn build_rows_without_session_are_draft() {
    let tasks = vec![make_task(1, "alpha")];
    let sessions: Vec<SessionInfo> = Vec::new();
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = crate::config::AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, _) = build_task_rows(&ctx, &tasks, &sessions, 0);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "Draft");
    assert_eq!(rows[0].session, None);
  }

  #[test]
  fn selection_zero_when_no_rows() {
    let tasks: Vec<TaskRef> = Vec::new();
    let sessions: Vec<SessionInfo> = Vec::new();
    let dir = tempfile::TempDir::new().expect("tmp");
    let ctx = crate::config::AppContext {
      paths: crate::config::AgencyPaths::new(dir.path()),
      config: crate::config::AgencyConfig::default(),
    };
    let (rows, sel) = build_task_rows(&ctx, &tasks, &sessions, 3);
    assert_eq!(rows.len(), 0);
    assert_eq!(sel, 0);
  }
}

/// UI events coming from daemon subscription
enum UiEvent {
  TasksChanged,
  SessionsChanged,
  Disconnected(Error),
}

fn subscribe_events(ctx: &AppContext) -> Result<Receiver<UiEvent>> {
  let (tx, rx) = unbounded::<UiEvent>();
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let project = ProjectKey {
    repo_root: repo_root.display().to_string(),
  };
  let stream = connect_daemon(ctx)?;
  std::thread::Builder::new()
    .name("tui-subscribe".to_string())
    .spawn(move || {
      let tx_events = tx;
      let mut stream = stream;
      if let Err(err) = write_frame(
        &mut stream,
        &C2D::Control(C2DControl::SubscribeEvents { project }),
      ) {
        let _ = tx_events.send(UiEvent::Disconnected(err));
        return;
      }
      loop {
        let msg: Result<D2C> = read_frame(&mut stream);
        match msg {
          Ok(D2C::Control(D2CControl::TasksChanged { .. })) => {
            let _ = tx_events.send(UiEvent::TasksChanged);
          }
          Ok(D2C::Control(D2CControl::SessionsChanged { .. })) => {
            let _ = tx_events.send(UiEvent::SessionsChanged);
          }
          Ok(_) => {}
          Err(err) => {
            let _ = tx_events.send(UiEvent::Disconnected(err));
            break;
          }
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

/// Build help lines from discrete items without breaking an item across lines.
fn layout_help_lines<'a>(items: &'a [&'a str], width: u16) -> Vec<Line<'a>> {
  let w = usize::from(width.max(1));
  let sep = " | ";
  let sep_len = sep.chars().count();
  let mut lines: Vec<Line> = Vec::new();
  let mut cur_len = 0_usize;
  let mut cur_spans: Vec<Span> = Vec::new();

  for item in items {
    let item_len = item.chars().count();
    if cur_len == 0 {
      cur_spans.push(Span::raw(*item));
      cur_len = item_len;
      continue;
    }

    if cur_len + sep_len + item_len <= w {
      cur_spans.push(Span::raw(sep));
      cur_spans.push(Span::raw(*item));
      cur_len += sep_len + item_len;
    } else {
      lines.push(Line::from(cur_spans));
      cur_spans = vec![Span::raw(*item)];
      cur_len = item_len;
    }
  }

  if !cur_spans.is_empty() {
    lines.push(Line::from(cur_spans));
  }
  lines
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Mode {
  #[default]
  List,
  InputSlug {
    start_and_attach: bool,
  },
}
