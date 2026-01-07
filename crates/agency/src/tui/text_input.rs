use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::layout::{centered_rect, inner};

/// Outcome of handling a key in the text input overlay.
#[derive(Clone, Debug)]
pub enum TextInputOutcome {
  /// Continue editing, no action taken.
  Continue,
  /// User submitted the input.
  Submit(String),
  /// User canceled the input.
  Canceled,
}

/// Configuration for the text input overlay.
#[derive(Clone, Debug)]
pub struct TextInputConfig {
  /// Title shown in the dialog border.
  pub title: String,
  /// Placeholder text shown when input is empty.
  pub placeholder: String,
  /// Optional right-side title (e.g., for showing selected options).
  pub right_title: Option<Line<'static>>,
}

impl TextInputConfig {
  /// Create a new text input configuration.
  pub fn new(title: impl Into<String>, placeholder: impl Into<String>) -> Self {
    Self {
      title: title.into(),
      placeholder: placeholder.into(),
      right_title: None,
    }
  }

  /// Set a right-aligned title.
  #[must_use]
  #[allow(dead_code)]
  pub fn with_right_title(mut self, title: Line<'static>) -> Self {
    self.right_title = Some(title);
    self
  }
}

/// Reusable text input overlay state.
#[derive(Clone, Debug)]
pub struct TextInputState {
  /// Current input text.
  pub input: String,
  /// Configuration for this input.
  pub config: TextInputConfig,
}

impl TextInputState {
  /// Create a new text input state with the given configuration.
  pub fn new(config: TextInputConfig) -> Self {
    Self {
      input: String::new(),
      config,
    }
  }

  /// Create with an initial value.
  #[allow(dead_code)]
  pub fn with_initial(config: TextInputConfig, initial: impl Into<String>) -> Self {
    Self {
      input: initial.into(),
      config,
    }
  }

  /// Draw the input overlay centered in the parent area.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: Rect) {
    let area = centered_rect(parent, 70, 5);
    let chunks = Layout::vertical([Constraint::Length(3)]).split(area);

    let mut block = Block::default()
      .borders(Borders::ALL)
      .title(Line::from(self.config.title.clone()));

    if let Some(ref right_title) = self.config.right_title {
      block = block.title(right_title.clone().right_aligned());
    }

    f.render_widget(block, chunks[0]);

    let input_area = inner(chunks[0]);
    let input_text = if self.input.is_empty() {
      Line::from(Span::raw(self.config.placeholder.clone()).fg(Color::Gray))
    } else {
      Line::from(self.input.clone())
    };
    f.render_widget(Paragraph::new(input_text), input_area);

    // Cursor placement
    let mut cx = input_area.x + u16::try_from(self.input.len()).unwrap_or(0);
    let max_x = input_area.x + input_area.width.saturating_sub(1);
    if cx > max_x {
      cx = max_x;
    }
    f.set_cursor_position((cx, input_area.y));
  }

  /// Handle key events. Returns a `TextInputOutcome` describing what happened.
  ///
  /// Note: This only handles basic text input keys (Esc, Enter, Backspace, Char).
  /// Callers can intercept other keys (like Ctrl+A) before calling this method.
  pub fn handle_key(&mut self, key: KeyEvent) -> TextInputOutcome {
    if key.kind == KeyEventKind::Repeat {
      return TextInputOutcome::Continue;
    }

    match key.code {
      KeyCode::Esc => TextInputOutcome::Canceled,
      KeyCode::Enter => TextInputOutcome::Submit(self.input.clone()),
      KeyCode::Backspace => {
        self.input.pop();
        TextInputOutcome::Continue
      }
      KeyCode::Char(c) => {
        self.input.push(c);
        TextInputOutcome::Continue
      }
      _ => TextInputOutcome::Continue,
    }
  }

  /// Update the right title (useful for showing dynamic state like selected agent).
  pub fn set_right_title(&mut self, title: Line<'static>) {
    self.config.right_title = Some(title);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn make_state() -> TextInputState {
    TextInputState::new(TextInputConfig::new("Title", "placeholder"))
  }

  #[test]
  fn esc_returns_canceled() {
    let mut state = make_state();
    let key = KeyEvent::from(KeyCode::Esc);
    assert!(matches!(state.handle_key(key), TextInputOutcome::Canceled));
  }

  #[test]
  fn enter_returns_submit_with_input() {
    let mut state = make_state();
    state.input = "test-value".to_string();
    let key = KeyEvent::from(KeyCode::Enter);
    match state.handle_key(key) {
      TextInputOutcome::Submit(val) => assert_eq!(val, "test-value"),
      _ => panic!("expected Submit"),
    }
  }

  #[test]
  fn typing_adds_chars() {
    let mut state = make_state();
    state.handle_key(KeyEvent::from(KeyCode::Char('a')));
    state.handle_key(KeyEvent::from(KeyCode::Char('b')));
    state.handle_key(KeyEvent::from(KeyCode::Char('c')));
    assert_eq!(state.input, "abc");
  }

  #[test]
  fn backspace_removes_char() {
    let mut state = make_state();
    state.input = "abc".to_string();
    state.handle_key(KeyEvent::from(KeyCode::Backspace));
    assert_eq!(state.input, "ab");
  }

  #[test]
  fn with_initial_sets_value() {
    let state = TextInputState::with_initial(
      TextInputConfig::new("Title", "placeholder"),
      "initial",
    );
    assert_eq!(state.input, "initial");
  }
}
