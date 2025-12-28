use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};

use super::text_input::{TextInputConfig, TextInputOutcome, TextInputState};
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
  text_input: TextInputState,
  pub selected_agent: Option<String>,
  pub start_and_attach: bool,
}

impl InputOverlayState {
  pub fn new(start_and_attach: bool, ctx: &AppContext) -> Self {
    let config = TextInputConfig::new("Task Slug", "my-task");
    let mut state = Self {
      text_input: TextInputState::new(config),
      selected_agent: default_agent(ctx),
      start_and_attach,
    };
    state.update_right_title();
    state
  }

  fn update_right_title(&mut self) {
    let agent_name = self
      .selected_agent
      .clone()
      .unwrap_or_else(|| "-".to_string());
    let right_title = Line::from(vec![
      Span::raw(agent_name).fg(Color::Cyan),
      Span::raw(" ("),
      Span::raw("C-a").fg(Color::Cyan),
      Span::raw(" to switch) "),
    ]);
    self.text_input.set_right_title(right_title);
  }

  /// Draw the input overlay centered in the parent area.
  pub fn draw(&self, f: &mut ratatui::Frame, parent: Rect) {
    self.text_input.draw(f, parent);
  }

  /// Handle key events. Returns an Action describing what happened.
  pub fn handle_key(&mut self, key: KeyEvent) -> Action {
    // Handle special keys first
    if key.code == KeyCode::Char('a') && key.modifiers.contains(KeyModifiers::CONTROL) {
      return Action::OpenAgentMenu;
    }

    match self.text_input.handle_key(key) {
      TextInputOutcome::Continue => Action::None,
      TextInputOutcome::Canceled => Action::Cancel,
      TextInputOutcome::Submit(input) => match normalize_and_validate_slug(&input) {
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
    }
  }

  /// Set the selected agent (called after agent menu selection).
  pub fn set_agent(&mut self, agent: String) {
    self.selected_agent = Some(agent);
    self.update_right_title();
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
