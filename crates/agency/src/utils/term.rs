use std::io::{self, Write, Read};
use anyhow::Result;

/// Soft-clear the current terminal view without losing scrollback.
///
/// This scrolls the viewport by a large amount (clamped by the terminal
/// height) and then moves the cursor to home. It keeps scrollback intact.
pub fn soft_reset_scroll() {
  let mut stdout = io::stdout().lock();
  let _ = stdout.write_all(b"\x1b[999S\x1b[H");
  let _ = stdout.flush();
}

/// Print a simple ASCII table to stdout.
/// Column widths are derived from headers and string lengths of rows.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
  let cols = headers.len();
  let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
  for row in rows {
    for (i, cell) in row.iter().enumerate().take(cols) {
      if cell.len() > widths[i] {
        widths[i] = cell.len();
      }
    }
  }

  // Header
  // Print header as single-space-separated tokens (no padding) to satisfy CLI tests
  for (i, h) in headers.iter().enumerate() {
    if i > 0 { print!(" "); }
    print!("{}", h);
  }
  println!();

  // Rows
  for row in rows {
    for i in 0..cols {
      let cell = row.get(i).map(String::as_str).unwrap_or("");
      let is_last = i + 1 == cols;
      if i == 0 {
        // First column (ID): right-align to header width, then space
        if is_last {
          let w = widths[i];
          print!("{:>width$}", cell, width = w);
        } else {
          let w = widths[i];
          print!("{:>width$} ", cell, width = w);
        }
      } else if i == 1 {
        // Second column (SLUG): no padding
        if is_last { print!("{}", cell); } else { print!("{} ", cell); }
      } else if is_last {
        print!("{}", cell);
      } else {
        let w = widths[i];
        print!("{:<width$} ", cell, width = w);
      }
    }
    println!();
  }
}

/// Ask the user to confirm an action. Returns true if input starts with 'y' or 'Y'.
pub fn confirm(prompt: &str) -> Result<bool> {
  let mut stdout = io::stdout().lock();
  let _ = write!(stdout, "{} ", prompt);
  let _ = stdout.flush();
  let mut line = String::new();
  let mut stdin = io::stdin().lock();
  // Read a single line (best-effort); empty or non-y -> false
  loop {
    let mut buf = [0u8; 1];
    match stdin.read(&mut buf) {
      Ok(0) => break,
      Ok(_) => {
        let b = buf[0];
        if b == b'\n' || b == b'\r' { break; }
        line.push(b as char);
        // Prevent overly long reads
        if line.len() > 100 { break; }
      }
      Err(_) => break,
    }
  }
  let ans = line.trim();
  Ok(matches!(ans.chars().next(), Some('y' | 'Y')))
}
