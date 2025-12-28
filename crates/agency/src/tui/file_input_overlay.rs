use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::layout::{centered_rect, inner};
use crate::utils::task::TaskRef;

/// Actions from the file input overlay.
#[derive(Clone, Debug)]
pub enum FileInputAction {
  None,
  Cancel,
  Submit { path: PathBuf },
}

/// State for the file path input overlay.
#[derive(Clone, Debug)]
pub struct FileInputState {
  pub path_input: String,
  pub task: TaskRef,
}

impl FileInputState {
  pub fn new(task: TaskRef) -> Self {
    Self {
      path_input: String::new(),
      task,
    }
  }

  /// Draw the input overlay centered in the parent area.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: Rect) {
    let area = centered_rect(parent, 70, 5);
    let chunks = Layout::vertical([Constraint::Length(3)]).split(area);

    let title = Line::from(vec![
      Span::raw("Add file to task "),
      Span::styled(&self.task.slug, ratatui::style::Style::default().fg(Color::Cyan)),
    ]);

    let block = Block::default()
      .borders(Borders::ALL)
      .title(title);
    f.render_widget(block, chunks[0]);

    let input_area = inner(chunks[0]);
    let input_text = if self.path_input.is_empty() {
      Line::from(Span::raw("/path/to/file").fg(Color::Gray))
    } else {
      Line::from(self.path_input.clone())
    };
    f.render_widget(Paragraph::new(input_text), input_area);

    // Cursor placement
    let mut cx = input_area.x + u16::try_from(self.path_input.len()).unwrap_or(0);
    let max_x = input_area.x + input_area.width.saturating_sub(1);
    if cx > max_x {
      cx = max_x;
    }
    f.set_cursor_position((cx, input_area.y));
  }

  /// Handle key events. Returns a `FileInputAction` describing what happened.
  pub fn handle_key(&mut self, key: KeyEvent) -> FileInputAction {
    match key.code {
      KeyCode::Esc => FileInputAction::Cancel,
      KeyCode::Enter => {
        if self.path_input.is_empty() {
          FileInputAction::None
        } else {
          FileInputAction::Submit {
            path: PathBuf::from(&self.path_input),
          }
        }
      }
      KeyCode::Backspace => {
        self.path_input.pop();
        FileInputAction::None
      }
      KeyCode::Char(c) => {
        self.path_input.push(c);
        FileInputAction::None
      }
      _ => FileInputAction::None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn make_state() -> FileInputState {
    FileInputState::new(TaskRef {
      id: 1,
      slug: "test".to_string(),
    })
  }

  #[test]
  fn cancel_on_esc() {
    let mut state = make_state();
    let key = KeyEvent::from(KeyCode::Esc);
    assert!(matches!(state.handle_key(key), FileInputAction::Cancel));
  }

  #[test]
  fn submit_with_path() {
    let mut state = make_state();
    state.path_input = "/tmp/test.txt".to_string();
    let key = KeyEvent::from(KeyCode::Enter);
    match state.handle_key(key) {
      FileInputAction::Submit { path } => {
        assert_eq!(path, PathBuf::from("/tmp/test.txt"));
      }
      _ => panic!("expected Submit"),
    }
  }

  #[test]
  fn no_submit_on_empty() {
    let mut state = make_state();
    let key = KeyEvent::from(KeyCode::Enter);
    assert!(matches!(state.handle_key(key), FileInputAction::None));
  }

  #[test]
  fn typing_adds_chars() {
    let mut state = make_state();
    state.handle_key(KeyEvent::from(KeyCode::Char('/')));
    state.handle_key(KeyEvent::from(KeyCode::Char('t')));
    state.handle_key(KeyEvent::from(KeyCode::Char('m')));
    state.handle_key(KeyEvent::from(KeyCode::Char('p')));
    assert_eq!(state.path_input, "/tmp");
  }

  #[test]
  fn backspace_removes_char() {
    let mut state = make_state();
    state.path_input = "/tmp".to_string();
    state.handle_key(KeyEvent::from(KeyCode::Backspace));
    assert_eq!(state.path_input, "/tm");
  }
}
