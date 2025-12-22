use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::layout::{centered_rect, inner};
use crate::config::AppContext;
use crate::log_error;
use crate::utils::task::normalize_and_validate_slug;

/// Actions from the input overlay.
#[derive(Clone, Debug)]
pub enum Action {
  None,
  Cancel,
  OpenAgentMenu,
  Submit {
    slug: String,
    agent: Option<String>,
    start_and_attach: bool,
  },
}

/// State for the new task input overlay.
pub struct InputOverlayState {
  pub slug_input: String,
  pub selected_agent: Option<String>,
  pub start_and_attach: bool,
}

impl InputOverlayState {
  pub fn new(start_and_attach: bool, ctx: &AppContext) -> Self {
    Self {
      slug_input: String::new(),
      selected_agent: default_agent(ctx),
      start_and_attach,
    }
  }

  /// Draw the input overlay centered in the parent area.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: Rect) {
    let area = centered_rect(parent, 70, 5);
    let chunks = Layout::vertical([Constraint::Length(3)]).split(area);

    let agent_name = self
      .selected_agent
      .clone()
      .unwrap_or_else(|| "-".to_string());
    let right_title = Line::from(vec![
      Span::raw(agent_name).fg(Color::Cyan),
      Span::raw(" ("),
      Span::raw("C-a").fg(Color::Cyan),
      Span::raw(" to switch) "),
    ])
    .right_aligned();

    let slug_block = Block::default()
      .borders(Borders::ALL)
      .title(Line::from("Task Slug"))
      .title(right_title);
    f.render_widget(slug_block, chunks[0]);

    let slug_area = inner(chunks[0]);
    let slug_text = if self.slug_input.is_empty() {
      Line::from(Span::raw("my-task").fg(Color::Gray))
    } else {
      Line::from(self.slug_input.clone())
    };
    f.render_widget(Paragraph::new(slug_text), slug_area);

    // Cursor placement
    let mut cx = slug_area.x + u16::try_from(self.slug_input.len()).unwrap_or(0);
    let max_x = slug_area.x + slug_area.width.saturating_sub(1);
    if cx > max_x {
      cx = max_x;
    }
    f.set_cursor_position((cx, slug_area.y));
  }

  /// Handle key events. Returns an Action describing what happened.
  pub fn handle_key(&mut self, key: KeyEvent) -> Action {
    match key.code {
      KeyCode::Esc => Action::Cancel,
      KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::OpenAgentMenu,
      KeyCode::Enter => match normalize_and_validate_slug(&self.slug_input) {
        Ok(slug) => Action::Submit {
          slug,
          agent: self.selected_agent.clone(),
          start_and_attach: self.start_and_attach,
        },
        Err(err) => {
          log_error!("New failed: {}", err);
          Action::None
        }
      },
      KeyCode::Backspace => {
        self.slug_input.pop();
        Action::None
      }
      KeyCode::Char(c) => {
        self.slug_input.push(c);
        Action::None
      }
      _ => Action::None,
    }
  }

  /// Set the selected agent (called after agent menu selection).
  pub fn set_agent(&mut self, agent: String) {
    self.selected_agent = Some(agent);
  }
}

/// Pick the default agent: config default if set, otherwise the first defined.
pub fn default_agent(ctx: &AppContext) -> Option<String> {
  ctx
    .config
    .agent
    .clone()
    .or_else(|| ctx.config.agents.keys().next().cloned())
}
