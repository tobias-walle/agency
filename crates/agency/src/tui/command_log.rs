use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::colors;
use crate::utils::log::LogEvent;

const MAX_LOG: usize = 200;

/// State for the command log pane.
#[derive(Default)]
pub struct CommandLogState {
  entries: Vec<LogEvent>,
  /// Scroll offset from bottom (0 = stick to latest).
  scroll: usize,
}

impl CommandLogState {
  pub fn new() -> Self {
    Self::default()
  }

  /// Push a log event, trimming old entries if over capacity.
  pub fn push(&mut self, ev: LogEvent) {
    self.entries.push(ev);
    if self.entries.len() > MAX_LOG {
      let overflow = self.entries.len() - MAX_LOG;
      self.entries.drain(0..overflow);
    }
  }

  /// Draw the command log pane.
  pub fn draw(&self, f: &mut ratatui::Frame, area: Rect, focused: bool) {
    let lines = self.build_lines();
    let title = if focused {
      Line::from("[2] Command Log").fg(Color::Cyan)
    } else {
      Line::from("[2] Command Log")
    };
    let block = Block::default().borders(Borders::ALL).title(title);

    let content_h = area.height.saturating_sub(2) as usize;
    let total_lines = lines.len();
    let start = if focused {
      compute_start(total_lines, content_h, self.scroll)
    } else {
      total_lines.saturating_sub(content_h)
    };
    let visible = lines[start..].to_vec();

    let para = Paragraph::new(visible).block(block);
    f.render_widget(para, area);
  }

  /// Handle key events for scrolling. Returns true if key was consumed.
  pub fn handle_key(&mut self, key: KeyEvent) -> bool {
    match key.code {
      KeyCode::Up | KeyCode::Char('k') => {
        self.scroll = self.scroll.saturating_add(1);
        true
      }
      KeyCode::Down | KeyCode::Char('j') => {
        self.scroll = self.scroll.saturating_sub(1);
        true
      }
      _ => false,
    }
  }

  /// Reset scroll to bottom.
  pub fn reset_scroll(&mut self) {
    self.scroll = 0;
  }

  fn build_lines(&self) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::with_capacity(self.entries.len());
    for ev in &self.entries {
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
    lines
  }
}

fn compute_start(total_lines: usize, content_h: usize, scroll: usize) -> usize {
  total_lines.saturating_sub(content_h.saturating_add(scroll))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn compute_log_start_bounds_and_offsets() {
    // When at bottom (scroll=0), show latest window
    assert_eq!(compute_start(100, 5, 0), 95);
    // Scroll up by 1 line
    assert_eq!(compute_start(100, 5, 1), 94);
    // Saturate at start when scroll exceeds available history
    assert_eq!(compute_start(3, 5, 10), 0);
    // No panic on zero content_h
    assert_eq!(compute_start(10, 0, 0), 10);
  }

  #[test]
  fn push_trims_old_entries() {
    let mut state = CommandLogState::new();
    for i in 0..250 {
      state.push(LogEvent::Command(format!("cmd {i}")));
    }
    assert_eq!(state.entries.len(), MAX_LOG);
  }
}
