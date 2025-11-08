use std::io::{self, IsTerminal as _};
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::prelude::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Terminal;

use crate::config::AppContext;
use crate::log_info;
use crate::pty::protocol::SessionInfo;
use crate::utils::daemon::list_sessions_for_project as list_sessions;
use crate::utils::task::{list_tasks, TaskRef, task_file};
use crate::utils::editor::open_path;

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
}

impl AppState {
  fn refresh(&mut self, ctx: &AppContext) -> Result<()> {
    let mut tasks = list_tasks(&ctx.paths)?;
    tasks.sort_by_key(|t| t.id);

    let sessions = list_sessions(ctx);
    let mut latest: std::collections::HashMap<(u32, String), SessionInfo> =
      std::collections::HashMap::new();
    for s in sessions {
      let key = (s.task.id, s.task.slug.clone());
      match latest.get(&key) {
        None => {
          latest.insert(key, s);
        }
        Some(prev) => {
          if s.created_at_ms >= prev.created_at_ms {
            latest.insert(key, s);
          }
        }
      }
    }

    let mut out = Vec::with_capacity(tasks.len());
    for t in tasks {
      if let Some(info) = latest.get(&(t.id, t.slug.clone())) {
        out.push(TaskRow {
          id: t.id,
          slug: t.slug,
          status: info.status.clone(),
          session: Some(info.session_id),
        });
      } else {
        out.push(TaskRow {
          id: t.id,
          slug: t.slug,
          status: "Draft".to_string(),
          session: None,
        });
      }
    }
    self.rows = out;
    if self.rows.is_empty() {
      self.selected = 0;
    } else {
      self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }
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
    let mut out = terminal.backend_mut();
    crossterm::execute!(out, crossterm::terminal::LeaveAlternateScreen)
      .ok();
    disable_raw_mode().ok();
    res
}

fn ui_loop(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, ctx: &AppContext) -> Result<()> {
  let mut state = AppState::default();
  state.refresh(ctx)?;

  loop {
    // Draw
    terminal.draw(|f| {
      let rects = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
      ]).split(f.area());

      // Table
      let header = Row::new([
        Cell::from("ID"),
        Cell::from("SLUG"),
        Cell::from("STATUS"),
        Cell::from("SESSION"),
      ]).style(Style::default().fg(Color::Gray));

      let rows = state.rows.iter().map(|r| {
        Row::new([
          Cell::from(r.id.to_string()),
          Cell::from(r.slug.clone()),
          Cell::from(r.status.clone()).style(status_style(&r.status)),
          Cell::from(r.session.map(|s| s.to_string()).unwrap_or_default()),
        ])
      });

      let table = Table::new(rows, [
        Constraint::Length(6),
        Constraint::Percentage(50),
        Constraint::Length(10),
        Constraint::Length(8),
      ])
      .header(header)
      .highlight_style(Style::default().bg(Color::DarkGray))
      .block(Block::default().borders(Borders::ALL).title("Tasks"));

      let mut tstate = ratatui::widgets::TableState::default();
      tstate.select(Some(state.selected));
      f.render_stateful_widget(table, rects[0], &mut tstate);

      // Help bar
      let help = Line::from("Select: ↑/↓ j/k | Edit/Attach: ⏎ | Quit: q").fg(Color::Blue);
      // No border for the help area -> avoids double line under the table
      f.render_widget(
        ratatui::widgets::Paragraph::new(help)
          .alignment(Alignment::Center),
        rects[1],
      );
    })?;

    // Events
    if event::poll(Duration::from_millis(150))? {
      if let Event::Key(key) = event::read()? {
        // Ignore key repeats
        if key.kind == KeyEventKind::Repeat { continue; }
        match key.code {
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
              // Leave TUI to run action
              restore_terminal(terminal)?;
              if let Some(sid) = cur.session {
                let _ = crate::commands::attach::run_join_session(ctx, sid);
              } else {
                // Draft -> open markdown task file in editor
                let tref = TaskRef { id: cur.id, slug: cur.slug.clone() };
                let tf = task_file(&ctx.paths, &tref);
                let _ = open_path(&tf);
              }
              // Re-init terminal and refresh
              reinit_terminal(terminal)?;
              state.refresh(ctx)?;
            }
          }
          _ => {}
        }
      }
    }
  }

  Ok(())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
  let mut out = terminal.backend_mut();
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
