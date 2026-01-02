use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Mode for the overlay display
pub enum OverlayMode<'a> {
  /// No task is selected (empty task list)
  NoTasks,
  /// A task is selected but has no running session (draft)
  NoSession {
    task_id: u32,
    slug: &'a str,
    description: &'a str,
  },
}

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

  pub fn draw(&mut self, mode: &OverlayMode) -> Result<()> {
    match mode {
      OverlayMode::NoTasks => self.draw_no_tasks(),
      OverlayMode::NoSession {
        task_id,
        slug,
        description,
      } => self.draw_no_session(*task_id, slug, description),
    }
  }

  fn draw_no_tasks(&mut self) -> Result<()> {
    self.terminal.draw(|f| {
      let area = f.area();
      let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
      ])
      .split(area);

      let lines = vec![
        Line::from(Span::styled(
          "No tasks. Create a task to continue.",
          Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled("Press C-c to cancel.", Style::default())),
      ];

      let para = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .alignment(Alignment::Center);
      f.render_widget(para, layout[1]);
    })?;
    Ok(())
  }

  fn draw_no_session(&mut self, task_id: u32, slug: &str, description: &str) -> Result<()> {
    self.terminal.draw(|f| {
      let area = f.area();

      // Calculate available height for description
      let desc_max_lines = area.height.saturating_sub(6).min(10) as usize;
      let truncated_desc = truncate_description(description, desc_max_lines, area.width as usize);

      let desc_height = u16::try_from(truncated_desc.lines().count().max(1)).unwrap_or(u16::MAX);

      let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),                 // Header
        Constraint::Length(1),                 // Spacer
        Constraint::Length(desc_height),       // Description
        Constraint::Length(1),                 // Spacer
        Constraint::Length(1),                 // Controls
        Constraint::Fill(1),
      ])
      .split(area);

      // Header
      let header = Line::from(vec![
        Span::styled("Task: ", Style::default()),
        Span::styled(slug, Style::default().fg(Color::Cyan)),
        Span::styled(format!(" (ID: {task_id})"), Style::default().fg(Color::DarkGray)),
      ]);
      let status = Line::from(Span::styled(
        "No session",
        Style::default().fg(Color::Yellow),
      ));

      let header_para = Paragraph::new(vec![header, Line::from(""), status])
        .alignment(Alignment::Center);
      f.render_widget(header_para, layout[1]);

      // Description
      if !truncated_desc.is_empty() {
        let desc_para = Paragraph::new(truncated_desc.as_str())
          .block(Block::default().borders(Borders::NONE))
          .alignment(Alignment::Center)
          .wrap(Wrap { trim: true })
          .style(Style::default().fg(Color::White));
        f.render_widget(desc_para, layout[3]);
      }

      // Controls
      let controls = Line::from(vec![
        Span::styled("Press ", Style::default()),
        Span::styled("'s'", Style::default().fg(Color::Green)),
        Span::styled(" to start, ", Style::default()),
        Span::styled("C-c", Style::default().fg(Color::Red)),
        Span::styled(" to cancel.", Style::default()),
      ]);
      let controls_para = Paragraph::new(controls).alignment(Alignment::Center);
      f.render_widget(controls_para, layout[5]);
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

/// Truncate description to fit within given lines and width
fn truncate_description(desc: &str, max_lines: usize, width: usize) -> String {
  if desc.is_empty() || max_lines == 0 {
    return String::new();
  }

  let trimmed = desc.trim();
  let lines: Vec<&str> = trimmed.lines().collect();

  // Simple truncation: take first max_lines lines
  let mut result: Vec<String> = Vec::new();
  let effective_width = width.saturating_sub(4).max(20); // Leave some margin

  for line in lines.iter().take(max_lines) {
    let truncated = if line.len() > effective_width {
      format!("{}...", &line[..effective_width.saturating_sub(3)])
    } else {
      (*line).to_string()
    };
    result.push(truncated);
  }

  if lines.len() > max_lines {
    // Replace last line with ellipsis indicator
    if !result.is_empty() {
      result.pop();
      result.push("...".to_string());
    }
  }

  result.join("\n")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn truncate_empty_returns_empty() {
    assert_eq!(truncate_description("", 10, 80), "");
  }

  #[test]
  fn truncate_zero_lines_returns_empty() {
    assert_eq!(truncate_description("some text", 0, 80), "");
  }

  #[test]
  fn truncate_single_line_fits() {
    let desc = "Short description";
    assert_eq!(truncate_description(desc, 5, 80), desc);
  }

  #[test]
  fn truncate_long_line_adds_ellipsis() {
    let desc = "x".repeat(100);
    let result = truncate_description(&desc, 5, 50);
    assert!(result.ends_with("..."));
    assert!(result.len() < 50);
  }

  #[test]
  fn truncate_respects_max_lines() {
    let desc = "line1\nline2\nline3\nline4\nline5";
    let result = truncate_description(desc, 3, 80);
    // Exceeds max_lines, so last line becomes "..."
    assert!(result.ends_with("..."));
    assert_eq!(result.lines().count(), 3);
  }

  #[test]
  fn truncate_exact_lines_no_ellipsis() {
    let desc = "line1\nline2\nline3";
    let result = truncate_description(desc, 3, 80);
    assert!(!result.ends_with("..."));
    assert_eq!(result, desc);
  }

  #[test]
  fn truncate_trims_whitespace() {
    let desc = "  \n  content  \n  ";
    let result = truncate_description(desc, 5, 80);
    assert_eq!(result, "content");
  }
}
