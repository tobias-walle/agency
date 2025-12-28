use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use crate::config::AgencyPaths;
use crate::utils::files::{FileRef, list_files};
use crate::utils::task::TaskRef;

use super::layout::centered_rect;

/// Outcome of handling a key while the files overlay is open.
pub enum FilesOutcome {
  Continue,
  OpenFile(FileRef),
  OpenDirectory,
  PasteClipboard,
  Canceled,
}

/// Overlay state for viewing files attached to a task.
#[derive(Clone, Debug)]
pub struct FilesOverlayState {
  pub task: TaskRef,
  pub files: Vec<FileRef>,
  pub selected: usize,
}

impl FilesOverlayState {
  /// Create a new files overlay for the given task.
  pub fn new(paths: &AgencyPaths, task: TaskRef) -> Self {
    let files = list_files(paths, &task).unwrap_or_default();
    Self {
      task,
      files,
      selected: 0,
    }
  }

  /// Draw the overlay centered within `parent`.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: ratatui::layout::Rect) {
    let title = Line::from(vec![
      Span::raw("Attached files of task "),
      Span::styled(&self.task.slug, Style::default().fg(Color::Cyan)),
    ]);

    // Height: files + borders (minimum 3 for empty state)
    let item_count = self.files.len().max(1);
    let visible_count: u16 = u16::try_from(item_count).unwrap_or(u16::MAX);
    let height_rows = visible_count.min(14).saturating_add(2);
    let area = centered_rect(parent, 60, height_rows);

    let list_items: Vec<ListItem> = if self.files.is_empty() {
      vec![ListItem::new(
        Line::from("No files attached").style(Style::default().fg(Color::Gray)),
      )]
    } else {
      self
        .files
        .iter()
        .map(|file| {
          let line = Line::from(vec![
            Span::styled(format!("{} ", file.id), Style::default().fg(Color::Gray)),
            Span::styled(file.name.clone(), Style::default().fg(Color::Cyan)),
          ]);
          ListItem::new(line)
        })
        .collect()
    };

    let block = Block::default()
      .borders(Borders::ALL)
      .title(title.alignment(Alignment::Left));

    let list = List::new(list_items)
      .highlight_style(Style::default().bg(Color::DarkGray))
      .block(block);

    let mut state = ratatui::widgets::ListState::default();
    if !self.files.is_empty() {
      state.select(Some(self.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
  }

  /// Process a key event. Returns a `FilesOutcome` describing the result.
  pub fn handle_key(&mut self, key: KeyEvent) -> FilesOutcome {
    if key.kind == KeyEventKind::Repeat {
      return FilesOutcome::Continue;
    }

    match key.code {
      KeyCode::Esc => FilesOutcome::Canceled,
      KeyCode::Up | KeyCode::Char('k') => {
        self.select_prev();
        FilesOutcome::Continue
      }
      KeyCode::Down | KeyCode::Char('j') => {
        self.select_next();
        FilesOutcome::Continue
      }
      KeyCode::Enter | KeyCode::Char('o') => self.open_selected(),
      KeyCode::Char('O') => FilesOutcome::OpenDirectory,
      KeyCode::Char('p') => FilesOutcome::PasteClipboard,
      KeyCode::Char(c) => self.handle_digit(c),
      _ => FilesOutcome::Continue,
    }
  }

  fn open_selected(&self) -> FilesOutcome {
    self
      .files
      .get(self.selected)
      .map_or(FilesOutcome::Continue, |file| {
        FilesOutcome::OpenFile(file.clone())
      })
  }

  fn handle_digit(&mut self, c: char) -> FilesOutcome {
    let Some(digit) = c.to_digit(10) else {
      return FilesOutcome::Continue;
    };
    // Find file by ID and select it
    if let Some(idx) = self.files.iter().position(|f| f.id == digit) {
      self.selected = idx;
    }
    FilesOutcome::Continue
  }

  fn select_prev(&mut self) {
    if self.files.is_empty() {
      return;
    }
    if self.selected == 0 {
      self.selected = self.files.len().saturating_sub(1);
    } else {
      self.selected = self.selected.saturating_sub(1);
    }
  }

  fn select_next(&mut self) {
    if self.files.is_empty() {
      return;
    }
    if self.selected >= self.files.len().saturating_sub(1) {
      self.selected = 0;
    } else {
      self.selected = self.selected.saturating_add(1);
    }
  }

}

#[cfg(test)]
mod tests {
  use super::*;
  use crossterm::event::KeyEvent;

  fn make_overlay_with_files() -> FilesOverlayState {
    let task = TaskRef {
      id: 1,
      slug: "test".to_string(),
    };
    let files = vec![
      FileRef {
        id: 1,
        name: "screenshot.png".to_string(),
      },
      FileRef {
        id: 2,
        name: "spec.pdf".to_string(),
      },
    ];
    FilesOverlayState {
      task,
      files,
      selected: 0,
    }
  }

  fn make_empty_overlay() -> FilesOverlayState {
    let task = TaskRef {
      id: 1,
      slug: "test".to_string(),
    };
    FilesOverlayState {
      task,
      files: Vec::new(),
      selected: 0,
    }
  }

  #[test]
  fn navigation_wraps_around() {
    let mut overlay = make_overlay_with_files();
    // 2 files

    // Start at 0, go up should wrap to 1 (last file)
    let ev = KeyEvent::from(KeyCode::Up);
    overlay.handle_key(ev);
    assert_eq!(overlay.selected, 1);

    // Go down should wrap back to 0
    let ev = KeyEvent::from(KeyCode::Down);
    overlay.handle_key(ev);
    assert_eq!(overlay.selected, 0);
  }

  #[test]
  fn vim_navigation_works() {
    let mut overlay = make_overlay_with_files();

    let ev = KeyEvent::from(KeyCode::Char('j'));
    overlay.handle_key(ev);
    assert_eq!(overlay.selected, 1);

    let ev = KeyEvent::from(KeyCode::Char('k'));
    overlay.handle_key(ev);
    assert_eq!(overlay.selected, 0);
  }

  #[test]
  fn enter_on_file_returns_open_file() {
    let mut overlay = make_overlay_with_files();
    overlay.selected = 0;

    let ev = KeyEvent::from(KeyCode::Enter);
    match overlay.handle_key(ev) {
      FilesOutcome::OpenFile(file) => {
        assert_eq!(file.id, 1);
        assert_eq!(file.name, "screenshot.png");
      }
      _ => panic!("expected OpenFile"),
    }
  }

  #[test]
  fn esc_returns_canceled() {
    let mut overlay = make_overlay_with_files();

    let ev = KeyEvent::from(KeyCode::Esc);
    match overlay.handle_key(ev) {
      FilesOutcome::Canceled => {}
      _ => panic!("expected Canceled"),
    }
  }

  #[test]
  fn o_opens_selected_file() {
    let mut overlay = make_overlay_with_files();
    overlay.selected = 1;

    let ev = KeyEvent::from(KeyCode::Char('o'));
    match overlay.handle_key(ev) {
      FilesOutcome::OpenFile(file) => {
        assert_eq!(file.id, 2);
        assert_eq!(file.name, "spec.pdf");
      }
      _ => panic!("expected OpenFile"),
    }
  }

  #[test]
  fn uppercase_o_opens_directory() {
    let mut overlay = make_overlay_with_files();

    let ev = KeyEvent::from(KeyCode::Char('O'));
    match overlay.handle_key(ev) {
      FilesOutcome::OpenDirectory => {}
      _ => panic!("expected OpenDirectory"),
    }
  }

  #[test]
  fn digit_selects_file_by_id() {
    let mut overlay = make_overlay_with_files();
    assert_eq!(overlay.selected, 0);

    let ev = KeyEvent::from(KeyCode::Char('2'));
    match overlay.handle_key(ev) {
      FilesOutcome::Continue => {
        assert_eq!(overlay.selected, 1); // File with id=2 is at index 1
      }
      _ => panic!("expected Continue"),
    }
  }

  #[test]
  fn empty_overlay_does_not_crash() {
    let mut overlay = make_empty_overlay();

    // Navigation should not crash
    let ev = KeyEvent::from(KeyCode::Down);
    overlay.handle_key(ev);
    assert_eq!(overlay.selected, 0);

    // Enter should not crash
    let ev = KeyEvent::from(KeyCode::Enter);
    match overlay.handle_key(ev) {
      FilesOutcome::Continue => {}
      _ => panic!("expected Continue for empty overlay"),
    }
  }
}
