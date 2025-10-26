use anstream::println;
use anyhow::Result;
use owo_colors::OwoColorize as _;
use regex::Regex;
use std::fmt::Display;
use std::io::{self, IsTerminal as _};
use std::sync::OnceLock;

/// Ask the user to confirm an action with a yes/no prompt.
///
/// Prints the `prompt` to stdout, reads a single line from stdin,
/// and returns `Ok(true)` if the trimmed input is `"y"` or `"Y"`.
/// Returns `Ok(false)` for any other input.
pub fn confirm(prompt: &str) -> Result<bool> {
  println!("{}", prompt);
  let mut input = String::new();
  io::stdin().read_line(&mut input)?;
  let trimmed = input.trim();
  Ok(trimmed == "y" || trimmed == "Y")
}

/// Strip ANSI color/style escape codes from a string.
fn strip_ansi(s: &str) -> String {
  static RE_ANSI: OnceLock<Regex> = OnceLock::new();
  let re = RE_ANSI.get_or_init(|| Regex::new(r"\x1B\[[0-9;]*m").expect("ansi regex"));
  re.replace_all(s, "").to_string()
}

/// Print a simple table with headers and rows to stdout.
///
/// - Computes column widths from headers and rows.
/// - Colors headers (cyan) when stdout is a TTY.
/// - Right-aligns numeric columns (detected by header text `ID`), left-aligns others.
/// - Avoids trailing spaces on the last column for cleaner output.
pub fn print_table(headers: &[impl Display], rows: &[Vec<String>]) {
  let s = format_table(headers, rows);
  for line in s.split('\n') {
    if !line.is_empty() {
      println!("{}", line);
    }
  }
}

/// Format a table into a string (with trailing newline).
///
/// This is used internally by `print_table` and in unit tests for
/// readable assertions.
fn format_table(headers: &[impl Display], rows: &[Vec<String>]) -> String {
  if headers.is_empty() {
    return String::new();
  }
  let cols = headers.len();
  let hdrs_raw: Vec<String> = headers.iter().map(|h| h.to_string()).collect();

  // Compute column widths based on stripped headers and rows
  let mut widths = vec![0_usize; cols];
  for i in 0..cols {
    let hw = strip_ansi(&hdrs_raw[i]).len();
    let rw = rows
      .iter()
       .map(|r| r.get(i).map(|c| strip_ansi(c).len()).unwrap_or(0))
      .max()
      .unwrap_or(0);
    widths[i] = hw.max(rw);
  }

  // Determine numeric columns (right align): header equals "ID"
  let numeric: Vec<bool> = (0..cols)
    .map(|i| strip_ansi(&hdrs_raw[i]).to_uppercase() == "ID")
    .collect();

  // Header line: color if stdout is a TTY; align; avoid padding last column
  let color_headers = io::stdout().is_terminal();
  let mut header_line_parts = Vec::with_capacity(cols);
  for i in 0..cols {
    let visible = strip_ansi(&hdrs_raw[i]);
    let text = if color_headers {
      format!("{}", visible.bold().dimmed())
    } else {
      visible
    };
    let pad = widths[i];
    let piece = if numeric[i] {
      let spaces = pad.saturating_sub(strip_ansi(&text).len());
      if i == cols - 1 {
        format!("{}{}", " ".repeat(spaces), text)
      } else {
        format!("{}{}", " ".repeat(spaces), text)
      }
    } else {
      let spaces = pad.saturating_sub(strip_ansi(&text).len());
      if i == cols - 1 {
        text
      } else {
        format!("{}{}", text, " ".repeat(spaces))
      }
    };
    header_line_parts.push(piece);
  }
  let header_line = header_line_parts.join(" ");

  // Body lines: align; do not pad the last column
  let mut body_lines = Vec::new();
  for row in rows.iter() {
    let mut parts = Vec::with_capacity(cols);
    for i in 0..cols {
      let val = row.get(i).cloned().unwrap_or_default();
      let pad = widths[i];
      if numeric[i] {
        let visible_len = strip_ansi(&val).len();
        let spaces = pad.saturating_sub(visible_len);
        if i == cols - 1 {
          parts.push(val);
        } else {
          parts.push(format!("{}{}", " ".repeat(spaces), val));
        }
      } else {
        let visible_len = strip_ansi(&val).len();
        let spaces = pad.saturating_sub(visible_len);
        if i == cols - 1 {
          parts.push(val);
        } else {
          parts.push(format!("{}{}", val, " ".repeat(spaces)));
        }
      }
    }
    body_lines.push(parts.join(" "));
  }

  let mut out = String::new();
  out.push_str(&header_line);
  out.push('\n');
  for line in body_lines {
    out.push_str(&line);
    out.push('\n');
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn formats_table_with_alignment() {
    let s = format_table(
      &["ID", "SLUG"],
      &vec![
        vec!["1".to_string(), "alpha-task".to_string()],
        vec!["12".to_string(), "beta".to_string()],
      ],
    );
    let expected = "ID SLUG\n 1 alpha-task\n12 beta\n";
    // Strip ANSI from output to compare reliably
    assert_eq!(strip_ansi(&s), expected);
  }

  #[test]
  fn prints_only_header_on_empty_rows() {
    let s = format_table(&["ID", "SLUG"], &Vec::new());
    let expected = "ID SLUG\n";
    assert_eq!(strip_ansi(&s), expected);
  }
}
