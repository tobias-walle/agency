use std::path::PathBuf;

use crossterm::event::KeyEvent;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::text_input::{TextInputConfig, TextInputOutcome, TextInputState};
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
  pub task: TaskRef,
  text_input: TextInputState,
}

impl FileInputState {
  pub fn new(task: TaskRef) -> Self {
    let title = Line::from(vec![
      Span::raw("Add file to task "),
      Span::styled(
        task.slug.clone(),
        ratatui::style::Style::default().fg(Color::Cyan),
      ),
    ]);
    let config = TextInputConfig {
      title: title.to_string(),
      placeholder: "/path/to/file".to_string(),
      right_title: None,
    };
    Self {
      task,
      text_input: TextInputState::new(config),
    }
  }

  /// Draw the input overlay centered in the parent area.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: Rect) {
    self.text_input.draw(f, parent);
  }

  /// Handle key events. Returns a `FileInputAction` describing what happened.
  pub fn handle_key(&mut self, key: KeyEvent) -> FileInputAction {
    match self.text_input.handle_key(key) {
      TextInputOutcome::Continue => FileInputAction::None,
      TextInputOutcome::Canceled => FileInputAction::Cancel,
      TextInputOutcome::Submit(path) => {
        if path.is_empty() {
          FileInputAction::None
        } else {
          FileInputAction::Submit {
            path: PathBuf::from(path),
          }
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crossterm::event::KeyCode;

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
    state.text_input.input = "/tmp/test.txt".to_string();
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
    assert_eq!(state.text_input.input, "/tmp");
  }

  #[test]
  fn backspace_removes_char() {
    let mut state = make_state();
    state.text_input.input = "/tmp".to_string();
    state.handle_key(KeyEvent::from(KeyCode::Backspace));
    assert_eq!(state.text_input.input, "/tm");
  }
}
