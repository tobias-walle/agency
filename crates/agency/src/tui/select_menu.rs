use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

use super::layout::centered_rect;

/// Outcome of handling a key while the menu is open
pub enum MenuOutcome {
  Continue,
  Selected(usize),
  Canceled,
}

/// Reusable overlay select menu state.
/// - Always appends a trailing "Cancel" item at render/interaction time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectMenuState {
  pub title: String,
  pub items: Vec<String>,
  pub selected: usize,
}

impl SelectMenuState {
  pub fn new<T: Into<String>>(title: T, items: Vec<String>, selected: usize) -> Self {
    let mut sel = selected;
    if items.is_empty() {
      sel = 0;
    } else if sel >= items.len() {
      sel = items.len() - 1;
    }
    Self {
      title: title.into(),
      items,
      selected: sel,
    }
  }

  /// Draw the overlayed menu centered within `parent`.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: ratatui::layout::Rect) {
    // Compute menu size: width 60%, height = min(items+Cancel, 12) + borders
    let visible_count: u16 = u16::try_from(self.items.len().saturating_add(1)).unwrap_or(u16::MAX); // + Cancel
    let height_rows = visible_count.min(12).saturating_add(2); // borders
    let area = centered_rect(parent, 60, height_rows);

    // Build list items (+ Cancel)
    let mut list_items: Vec<ListItem> = Vec::with_capacity(self.items.len() + 1);
    for (idx, it) in self.items.iter().enumerate() {
      let prefix = homerow_key_for(idx)
        .map(|c| format!("{c} "))
        .unwrap_or_default();
      let name = Span::styled(it.clone(), Style::default().fg(Color::Cyan));
      let line = Line::from(vec![Span::raw(prefix), name]);
      list_items.push(ListItem::new(line));
    }
    list_items.push(ListItem::new(
      Line::from("Cancel").style(Style::default().fg(Color::Gray)),
    ));

    // Clamp selected to items range (not including Cancel); when Cancel would
    // be selected via arrows, we still allow it using Enter handling below.
    let selected = self.selected.min(self.items.len());

    // Render
    let block = Block::default()
      .borders(Borders::ALL)
      .title(Line::from(self.title.clone()).alignment(Alignment::Left));
    let list = List::new(list_items)
      .highlight_style(Style::default().bg(Color::DarkGray))
      .block(block);
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
  }

  /// Process a key event. Returns a `MenuOutcome` describing the result.
  pub fn handle_key(&mut self, key: KeyEvent) -> MenuOutcome {
    if key.kind == KeyEventKind::Repeat {
      return MenuOutcome::Continue;
    }
    match key.code {
      KeyCode::Esc => return MenuOutcome::Canceled,
      KeyCode::Up => self.select_prev(),
      KeyCode::Down => self.select_next(),
      KeyCode::Enter => {
        // If selection points to Cancel (index == items.len()) then cancel
        if self.selected >= self.items.len() {
          return MenuOutcome::Canceled;
        }
        return MenuOutcome::Selected(self.selected);
      }
      KeyCode::Char(c) => {
        // Ctrl-<key> should not be treated as a homerow mnemonic
        if key.modifiers.contains(KeyModifiers::CONTROL) {
          return MenuOutcome::Continue;
        }
        // Vim-style navigation
        if c == 'k' {
          self.select_prev();
          return MenuOutcome::Continue;
        }
        if c == 'j' {
          self.select_next();
          return MenuOutcome::Continue;
        }
        if let Some(idx) = match_homerow_index(c)
          && idx < self.items.len()
        {
          self.selected = idx;
          return MenuOutcome::Selected(idx);
        }
      }
      _ => {}
    }
    MenuOutcome::Continue
  }

  #[inline]
  fn select_prev(&mut self) {
    if self.selected == 0 {
      self.selected = self.items.len();
    } else {
      self.selected -= 1;
    }
  }

  #[inline]
  fn select_next(&mut self) {
    if self.selected >= self.items.len() {
      self.selected = 0;
    } else {
      self.selected += 1;
    }
  }
}

/// Map indices to homerow mnemonic keys, excluding j/k to avoid conflicts.
/// 0->'a', 1->'s', 2->'d', 3->'f', 4->'l'
pub fn homerow_key_for(index: usize) -> Option<char> {
  match index {
    0 => Some('a'),
    1 => Some('s'),
    2 => Some('d'),
    3 => Some('f'),
    4 => Some('l'),
    _ => None,
  }
}

fn match_homerow_index(c: char) -> Option<usize> {
  match c {
    'a' => Some(0),
    's' => Some(1),
    'd' => Some(2),
    'f' => Some(3),
    'l' => Some(4),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crossterm::event::KeyEvent;

  #[test]
  fn homerow_mapping_excludes_jk() {
    assert_eq!(homerow_key_for(0), Some('a'));
    assert_eq!(homerow_key_for(1), Some('s'));
    assert_eq!(homerow_key_for(2), Some('d'));
    assert_eq!(homerow_key_for(3), Some('f'));
    assert_eq!(homerow_key_for(4), Some('l'));
    assert_eq!(homerow_key_for(5), None);
  }

  #[test]
  fn handle_key_homerow_and_cancel() {
    let mut menu = SelectMenuState::new("Test", vec!["one".into(), "two".into()], 0);
    // press 's' -> index 1
    let ev = KeyEvent::from(KeyCode::Char('s'));
    match menu.handle_key(ev) {
      MenuOutcome::Selected(idx) => assert_eq!(idx, 1),
      _ => panic!("expected Selected"),
    }

    // press Enter on Cancel
    menu.selected = menu.items.len();
    let ev2 = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    match menu.handle_key(ev2) {
      MenuOutcome::Canceled => {}
      _ => panic!("expected Canceled"),
    }
  }
}
