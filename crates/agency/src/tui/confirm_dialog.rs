use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Alignment;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::layout::centered_rect;

/// Outcome of handling a key while the confirm dialog is open.
pub enum ConfirmOutcome {
  Continue,
  Confirmed,
  Canceled,
}

/// Actions that can be confirmed via the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
  CompleteTask { id: u32 },
}

impl ConfirmAction {
  /// Command string to log when this action is executed.
  #[must_use]
  pub fn command_log(&self) -> String {
    match self {
      ConfirmAction::CompleteTask { id } => format!("agency complete {id}"),
    }
  }

  /// Task ID associated with this action.
  #[must_use]
  pub fn task_id(&self) -> u32 {
    match self {
      ConfirmAction::CompleteTask { id } => *id,
    }
  }
}

/// Which button is selected in the confirm dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConfirmSelection {
  #[default]
  Yes,
  No,
}

impl ConfirmSelection {
  fn toggle(self) -> Self {
    match self {
      Self::Yes => Self::No,
      Self::No => Self::Yes,
    }
  }
}

/// Reusable confirmation dialog state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfirmDialogState {
  pub title: String,
  pub message: String,
  pub action: ConfirmAction,
  pub selected: ConfirmSelection,
}

impl ConfirmDialogState {
  pub fn new<T, M>(title: T, message: M, action: ConfirmAction) -> Self
  where
    T: Into<String>,
    M: Into<String>,
  {
    Self {
      title: title.into(),
      message: message.into(),
      action,
      selected: ConfirmSelection::Yes,
    }
  }

  /// Draw the confirmation dialog centered within `parent`.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: ratatui::layout::Rect) {
    // Dialog dimensions: 50% width, 5 rows (border + message + spacer + buttons + border)
    let area = centered_rect(parent, 50, 5);

    // Clear the area behind the dialog
    f.render_widget(Clear, area);

    let block = Block::default()
      .borders(Borders::ALL)
      .title(Line::from(self.title.clone()).alignment(Alignment::Left))
      .border_style(Style::default().fg(Color::Yellow));

    let yes_style = if self.selected == ConfirmSelection::Yes {
      Style::default().fg(Color::Green).bg(Color::DarkGray)
    } else {
      Style::default().fg(Color::Green)
    };
    let no_style = if self.selected == ConfirmSelection::No {
      Style::default().fg(Color::Red).bg(Color::DarkGray)
    } else {
      Style::default().fg(Color::Red)
    };

    let content = vec![
      Line::from(self.message.clone()),
      Line::from(""),
      Line::from(vec![
        ratatui::text::Span::styled("  [Y]es  ", yes_style),
        ratatui::text::Span::raw("    "),
        ratatui::text::Span::styled("  [N]o  ", no_style),
      ]),
    ];

    let paragraph = Paragraph::new(content)
      .block(block)
      .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
  }

  /// Process a key event. Returns a `ConfirmOutcome` describing the result.
  pub fn handle_key(&mut self, key: KeyEvent) -> ConfirmOutcome {
    if key.kind == KeyEventKind::Repeat {
      return ConfirmOutcome::Continue;
    }

    match key.code {
      KeyCode::Esc | KeyCode::Char('n' | 'N') => ConfirmOutcome::Canceled,
      KeyCode::Char('y' | 'Y') => ConfirmOutcome::Confirmed,
      KeyCode::Enter => match self.selected {
        ConfirmSelection::Yes => ConfirmOutcome::Confirmed,
        ConfirmSelection::No => ConfirmOutcome::Canceled,
      },
      KeyCode::Left | KeyCode::Char('h') => {
        self.selected = ConfirmSelection::Yes;
        ConfirmOutcome::Continue
      }
      KeyCode::Right | KeyCode::Char('l') => {
        self.selected = ConfirmSelection::No;
        ConfirmOutcome::Continue
      }
      KeyCode::Tab => {
        self.selected = self.selected.toggle();
        ConfirmOutcome::Continue
      }
      _ => ConfirmOutcome::Continue,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crossterm::event::KeyModifiers;

  fn make_dialog() -> ConfirmDialogState {
    ConfirmDialogState::new("Title", "Message", ConfirmAction::CompleteTask { id: 42 })
  }

  #[test]
  fn confirm_dialog_initial_state() {
    let dialog = make_dialog();
    assert_eq!(dialog.title, "Title");
    assert_eq!(dialog.message, "Message");
    assert_eq!(dialog.action, ConfirmAction::CompleteTask { id: 42 });
    assert_eq!(dialog.selected, ConfirmSelection::Yes);
  }

  #[test]
  fn confirm_action_command_log() {
    let action = ConfirmAction::CompleteTask { id: 42 };
    assert_eq!(action.command_log(), "agency complete 42");
  }

  #[test]
  fn confirm_action_task_id() {
    let action = ConfirmAction::CompleteTask { id: 42 };
    assert_eq!(action.task_id(), 42);
  }

  #[test]
  fn confirm_dialog_y_confirms() {
    let mut dialog = make_dialog();
    let key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::empty());
    assert!(matches!(dialog.handle_key(key), ConfirmOutcome::Confirmed));
  }

  #[test]
  fn confirm_dialog_n_cancels() {
    let mut dialog = make_dialog();
    let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::empty());
    assert!(matches!(dialog.handle_key(key), ConfirmOutcome::Canceled));
  }

  #[test]
  fn confirm_dialog_esc_cancels() {
    let mut dialog = make_dialog();
    let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::empty());
    assert!(matches!(dialog.handle_key(key), ConfirmOutcome::Canceled));
  }

  #[test]
  fn confirm_dialog_enter_on_yes_confirms() {
    let mut dialog = make_dialog();
    dialog.selected = ConfirmSelection::Yes;
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    assert!(matches!(dialog.handle_key(key), ConfirmOutcome::Confirmed));
  }

  #[test]
  fn confirm_dialog_enter_on_no_cancels() {
    let mut dialog = make_dialog();
    dialog.selected = ConfirmSelection::No;
    let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::empty());
    assert!(matches!(dialog.handle_key(key), ConfirmOutcome::Canceled));
  }

  #[test]
  fn confirm_dialog_arrows_change_selection() {
    let mut dialog = make_dialog();
    assert_eq!(dialog.selected, ConfirmSelection::Yes);

    let right = KeyEvent::new(KeyCode::Right, KeyModifiers::empty());
    dialog.handle_key(right);
    assert_eq!(dialog.selected, ConfirmSelection::No);

    let left = KeyEvent::new(KeyCode::Left, KeyModifiers::empty());
    dialog.handle_key(left);
    assert_eq!(dialog.selected, ConfirmSelection::Yes);
  }

  #[test]
  fn confirm_dialog_tab_toggles_selection() {
    let mut dialog = make_dialog();
    assert_eq!(dialog.selected, ConfirmSelection::Yes);

    let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
    dialog.handle_key(tab);
    assert_eq!(dialog.selected, ConfirmSelection::No);

    let tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::empty());
    dialog.handle_key(tab);
    assert_eq!(dialog.selected, ConfirmSelection::Yes);
  }
}
