use anyhow::Result;
use owo_colors::OwoColorize as _;
use regex::Regex;
use std::io::{self, Read, Write};
use std::sync::OnceLock;

fn ansi_regex() -> &'static Regex {
  static ANSI_RE: OnceLock<Regex> = OnceLock::new();
  ANSI_RE.get_or_init(|| {
    Regex::new(
      r"(?x)
      \x1B\[[0-?]*[ -/]*[@-~]    # CSI sequence
      |                            # or
      \x1B\][^\x07\x1B]*(?:\x07|\x1B\\)  # OSC sequence terminated by BEL or ST
    ",
    )
    .expect("valid ANSI regex")
  })
}

/// Soft-clear viewport and disable extended keyboard modes.
pub fn restore_terminal_state() {
  let mut stdout = io::stdout().lock();
  let _ = stdout.write_all(b"\x1b[999S\x1b[H");
  let _ = stdout.write_all(b"\x1b[>0u\x1b[?2004l\x1b[?1004l");
  let _ = stdout.flush();
}

/// Clear scrollback and viewport to avoid interleaving histories before printing a full transcript.
/// Uses CSI 3J to clear scrollback, CSI 2J to clear the screen, and homes the cursor.
pub fn clear_terminal_scrollback() {
  let mut stdout = io::stdout().lock();
  let _ = stdout.write_all(b"\x1b[3J\x1b[2J\x1b[H");
  let _ = stdout.flush();
}

/// Print a simple ASCII table to stdout.
/// Column widths are derived from headers and string lengths of rows.
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
  let cols = headers.len();
  // 1) Measure max width per column across header and values (visible length)
  let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
  for row in rows {
    for (i, cell) in row.iter().enumerate().take(cols) {
      let vlen = visible_len(cell);
      if vlen > widths[i] {
        widths[i] = vlen;
      }
    }
  }

  // 2) Render headers: header + spaces(col_max - header.len + 1) between columns
  let mut header_line = String::new();
  for (i, text) in headers.iter().enumerate() {
    header_line.push_str(text);
    if i + 1 < cols {
      let spaces = widths[i].saturating_sub(text.len()) + 1;
      header_line.push_str(&" ".repeat(spaces));
    }
  }
  println!("{}", header_line.dimmed());

  // 3) Render rows with the same spacing rule based on visible lengths
  for row in rows {
    for (i, cell) in row.iter().enumerate().take(cols) {
      let cell = cell.as_str();
      let vlen = visible_len(cell);
      print!("{cell}");
      if i + 1 < cols {
        let spaces = widths[i].saturating_sub(vlen) + 1;
        for _ in 0..spaces {
          print!(" ");
        }
      }
    }
    println!();
  }
}

/// Ask the user to confirm an action. Returns true if input starts with 'y' or 'Y'.
pub fn confirm(prompt: &str) -> Result<bool> {
  let mut stdout = io::stdout().lock();
  write!(stdout, "{prompt} ")?;
  stdout.flush()?;
  let mut line = String::new();
  let mut stdin = io::stdin().lock();
  // Read a single line (best-effort); empty or non-y -> false
  loop {
    let mut buf = [0u8; 1];
    match stdin.read(&mut buf) {
      Ok(0) => break,
      Ok(_) => {
        let b = buf[0];
        if b == b'\n' || b == b'\r' {
          break;
        }
        line.push(b as char);
        // Prevent overly long reads
        if line.len() > 100 {
          break;
        }
      }
      Err(err) => return Err(err.into()),
    }
  }
  let ans = line.trim();
  Ok(matches!(ans.chars().next(), Some('y' | 'Y')))
}

pub fn strip_ansi_control_codes(input: &str) -> String {
  ansi_regex().replace_all(input, "").into_owned()
}

fn visible_len(s: &str) -> usize {
  // Strip ANSI CSI and OSC sequences, then count remaining characters.
  let plain = strip_ansi_control_codes(s);
  plain.chars().count()
}
