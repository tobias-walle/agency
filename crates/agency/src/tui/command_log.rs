use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::colors;
use crate::utils::log::LogEvent;

const MAX_LOG: usize = 200;
/// Minimum command log height (1 content line + 2 borders).
pub const MIN_LOG_HEIGHT: u16 = 3;
/// Default command log height (5 content lines + 2 borders).
pub const DEFAULT_LOG_HEIGHT: u16 = 7;
/// Terminal height threshold for auto-collapse.
pub const AUTO_COLLAPSE_THRESHOLD: u16 = 15;

/// State for the command log pane.
pub struct CommandLogState {
  // Content
  entries: Vec<LogEvent>,
  /// Scroll offset from bottom (0 = stick to latest).
  scroll: usize,
  // Layout/resizing
  /// User's preferred height (in rows, including borders).
  height: u16,
  /// Whether the command log is visible (toggled with 'H').
  visible: bool,
  /// Whether user is currently dragging the log border.
  dragging: bool,
  /// Cached Y position of the command log top border for hit detection.
  border_y: u16,
}

impl Default for CommandLogState {
  fn default() -> Self {
    Self {
      entries: Vec::new(),
      scroll: 0,
      height: DEFAULT_LOG_HEIGHT,
      visible: true,
      dragging: false,
      border_y: 0,
    }
  }
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

  /// Calculate effective height considering visibility, auto-collapse, and bounds.
  pub fn effective_height(&self, terminal_height: u16, help_rows: u16) -> u16 {
    if !self.visible {
      return 0;
    }
    // Auto-collapse to minimum when terminal is small
    if terminal_height <= AUTO_COLLAPSE_THRESHOLD {
      return MIN_LOG_HEIGHT;
    }
    // Ensure task table gets at least MIN_LOG_HEIGHT rows
    let max_log_height = terminal_height.saturating_sub(help_rows + MIN_LOG_HEIGHT);
    self.height.clamp(MIN_LOG_HEIGHT, max_log_height)
  }

  /// Cache border Y position from layout for mouse hit detection.
  pub fn set_border_y(&mut self, y: u16) {
    self.border_y = y;
  }

  /// Toggle visibility of the command log.
  pub fn toggle_visibility(&mut self) {
    self.visible = !self.visible;
  }

  /// Check if command log is visible.
  pub fn is_visible(&self) -> bool {
    self.visible
  }

  /// Handle mouse events for border dragging.
  pub fn handle_mouse_event(&mut self, mouse: MouseEvent) {
    match mouse.kind {
      MouseEventKind::Down(MouseButton::Left) => {
        // Check if click is on the command log border (top row of command log area)
        if mouse.row == self.border_y && self.visible {
          self.dragging = true;
        }
      }
      MouseEventKind::Drag(MouseButton::Left) => {
        if self.dragging {
          // Calculate new height based on mouse Y position
          let delta = i32::from(self.border_y) - i32::from(mouse.row);
          let new_height = i32::from(self.height) + delta;
          self.height =
            u16::try_from(new_height.max(i32::from(MIN_LOG_HEIGHT))).unwrap_or(u16::MAX);
        }
      }
      MouseEventKind::Up(MouseButton::Left) => {
        self.dragging = false;
      }
      _ => {}
    }
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

  #[test]
  fn effective_height_returns_zero_when_hidden() {
    let mut state = CommandLogState::new();
    state.visible = false;
    assert_eq!(state.effective_height(30, 2), 0);
  }

  #[test]
  fn effective_height_auto_collapse_on_small_terminal() {
    let state = CommandLogState::new();
    // Terminal height at threshold
    assert_eq!(state.effective_height(AUTO_COLLAPSE_THRESHOLD, 2), MIN_LOG_HEIGHT);
    // Terminal height below threshold
    assert_eq!(state.effective_height(10, 2), MIN_LOG_HEIGHT);
  }

  #[test]
  fn effective_height_clamps_to_bounds() {
    let mut state = CommandLogState::new();
    // Normal case - returns stored height
    assert_eq!(state.effective_height(30, 2), DEFAULT_LOG_HEIGHT);
    // Clamped to minimum
    state.height = 1;
    assert_eq!(state.effective_height(30, 2), MIN_LOG_HEIGHT);
    // Clamped to maximum (leave room for task table)
    state.height = 100;
    let max = 30 - 2 - MIN_LOG_HEIGHT; // terminal - help - min_task_table
    assert_eq!(state.effective_height(30, 2), max);
  }

  #[test]
  fn toggle_visibility_flips_state() {
    let mut state = CommandLogState::new();
    assert!(state.is_visible());
    state.toggle_visibility();
    assert!(!state.is_visible());
    state.toggle_visibility();
    assert!(state.is_visible());
  }
}
