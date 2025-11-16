use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};

/// Minimal ratatui overlay used by `attach follow` when the focused task
/// does not yet have a running session. It owns raw mode and the alternate
/// screen during its lifetime.
pub struct OverlayUI {
  terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
}

impl OverlayUI {
  pub fn init() -> Result<Self> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
      stdout,
      crossterm::terminal::EnterAlternateScreen,
      crossterm::cursor::Hide
    )?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(Self { terminal })
  }

  pub fn draw(&mut self, slug: &str, task_id: u32) -> Result<()> {
    let msg =
      format!("No session for Task {slug} (ID: {task_id}). Press 's' to start, C-c to cancel.");
    self.terminal.draw(|f| {
      let area = f.area();
      let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
      ])
      .split(area);
      let block = Block::default().borders(Borders::NONE);
      let para = Paragraph::new(msg.clone())
        .block(block)
        .alignment(Alignment::Center)
        .style(Style::default());
      f.render_widget(para, layout[1]);
    })?;
    Ok(())
  }

  pub fn restore(mut self) {
    let out = self.terminal.backend_mut();
    let _ = crossterm::execute!(
      out,
      crossterm::terminal::LeaveAlternateScreen,
      crossterm::cursor::Show
    );
    disable_raw_mode().ok();
    crate::utils::term::restore_terminal_state();
  }
}
