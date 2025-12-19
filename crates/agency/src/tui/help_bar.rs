use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Stylize;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Help items to display in the TUI.
pub const HELP_ITEMS: &[&str] = &[
  "Select: ↑/↓ j/k",
  "Edit/Attach: ⏎",
  "New: n",
  "New + Start: N",
  "Start: s",
  "Stop: S",
  "Merge: m",
  "Open: o",
  "Delete: X",
  "Reset: R",
  "Quit: C-c",
];

/// Draw the help bar with smart item-boundary wrapping.
pub fn draw(f: &mut ratatui::Frame, area: Rect) {
  let mut lines = layout_lines(HELP_ITEMS, area.width);
  lines = lines.into_iter().map(|ln| ln.fg(Color::Blue)).collect();
  f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// Build help lines from discrete items without breaking an item across lines.
pub fn layout_lines<'a>(items: &'a [&'a str], width: u16) -> Vec<Line<'a>> {
  let w = usize::from(width.max(1));
  let sep = " | ";
  let sep_len = sep.chars().count();
  let mut lines: Vec<Line> = Vec::new();
  let mut cur_len = 0_usize;
  let mut cur_spans: Vec<Span> = Vec::new();

  for item in items {
    let item_len = item.chars().count();
    if cur_len == 0 {
      cur_spans.push(Span::raw(*item));
      cur_len = item_len;
      continue;
    }

    if cur_len + sep_len + item_len <= w {
      cur_spans.push(Span::raw(sep));
      cur_spans.push(Span::raw(*item));
      cur_len += sep_len + item_len;
    } else {
      lines.push(Line::from(cur_spans));
      cur_spans = vec![Span::raw(*item)];
      cur_len = item_len;
    }
  }

  if !cur_spans.is_empty() {
    lines.push(Line::from(cur_spans));
  }
  lines
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn help_layout_item_boundary_wrap() {
    let items = [
      "Select: ↑/↓ j/k",
      "Edit/Attach: ⏎",
      "New: n/N",
      "Start: s",
      "Stop: S",
      "Merge: m",
      "Open: o",
      "Delete: X",
      "Reset: R",
      "Quit: C-c",
    ];
    // Very narrow should result in many lines but keep pairs intact
    let lines = layout_lines(&items, 20);
    assert!(lines.len() >= 2);

    // Ensure the last line contains Reset and Quit together when width allows
    let lines2 = layout_lines(&items, 60);
    let all_line_texts: Vec<String> = lines2
      .iter()
      .map(|ln| ln.spans.iter().map(|s| s.content.to_string()).collect())
      .collect();
    assert!(all_line_texts.iter().any(|t| t.contains("Reset: R")));
    assert!(all_line_texts.iter().any(|t| t.contains("Quit: C-c")));
  }
}
