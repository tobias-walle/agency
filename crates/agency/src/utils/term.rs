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

/// Same as `restore_terminal_state` but also clears scrollback to start fresh.
pub fn restore_terminal_state_and_clear_scrollback() {
  clear_terminal_scrollback();
  force_primary_screen_and_modes_off(&mut io::stdout().lock());
}

/// After printing a transcript, force the terminal into a known, safe state.
/// - Leave alternate screen if any sequence toggled it on
/// - Disable bracketed paste and focus tracking
/// - Show cursor
/// - Reset modifyOtherKeys (>0u)
pub fn force_primary_screen_and_modes_off(out: &mut dyn Write) {
  // Leave all private modes we commonly strip during sanitization
  for n in STRIP_PRIVATE_MODES_DEFAULT {
    let _ = write!(out, "\x1b[?{}l", n);
  }
  // Reset modifyOtherKeys and show cursor
  let _ = out.write_all(b"\x1b[>0u\x1b[?25h");
  let _ = out.flush();
}

/// DEC private-mode parameters that are stripped by default during attach
/// snapshot sanitization. Each entry is documented:
/// - 1049: Enter/leave alternate screen with scrollback swap (common for TUIs)
/// - 1047: Enter/leave alternate screen (older variant)
/// - 47:   Legacy alternate screen toggle
/// - 2004: Bracketed paste mode
/// - 1004: Focus in/out reporting
pub const STRIP_PRIVATE_MODES_DEFAULT: &[u32] = &[1049, 1047, 47, 2004, 1004];

/// Full-clear markers used to trim transcript to the content after the last
/// screen reset. Each marker is the exact byte sequence to match.
/// - ESC[2J: Clear screen
/// - ESC[3J: Clear screen and scrollback
/// - ESC c:  RIS (full reset)
/// - ESC[!p: DECSTR (soft reset)
pub const FULL_CLEAR_MARKERS_DEFAULT: &[&[u8]] = &[
  b"\x1b[2J",
  b"\x1b[3J",
  b"\x1bc",
  b"\x1b[!p",
];

/// Configuration for transcript sanitization.
pub struct TranscriptSanitizeSpec<'a> {
  pub strip_private_modes: &'a [u32],
  pub full_clear_markers: &'a [&'a [u8]],
  pub trim_to_last_full_clear: bool,
}

impl<'a> TranscriptSanitizeSpec<'a> {
  pub fn defaults() -> Self {
    Self {
      strip_private_modes: STRIP_PRIVATE_MODES_DEFAULT,
      full_clear_markers: FULL_CLEAR_MARKERS_DEFAULT,
      trim_to_last_full_clear: true,
    }
  }
}

/// Convenience wrapper using default spec.
pub fn sanitize_transcript(bytes: &[u8]) -> Vec<u8> {
  let spec = TranscriptSanitizeSpec::defaults();
  sanitize_transcript_with(bytes, &spec)
}

/// Sanitize transcript with a custom spec: optionally trim to last full-clear
/// marker and strip configured DEC private-mode toggles.
pub fn sanitize_transcript_with(bytes: &[u8], spec: &TranscriptSanitizeSpec) -> Vec<u8> {
  // Optional trimming to the last full-clear marker
  let slice = if spec.trim_to_last_full_clear {
    let mut last_end: Option<usize> = None;
    for marker in spec.full_clear_markers {
      let mut pos = 0;
      while let Some(idx) = find_subslice(&bytes[pos..], marker) {
        let end = pos + idx + marker.len();
        last_end = Some(last_end.map_or(end, |cur| cur.max(end)));
        pos += idx + marker.len();
      }
    }
    match last_end {
      Some(end) if end < bytes.len() => &bytes[end..],
      _ => bytes,
    }
  } else {
    bytes
  };

  strip_private_modes(slice, spec.strip_private_modes)
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
  if needle.is_empty() || hay.len() < needle.len() {
    return None;
  }
  hay.windows(needle.len()).position(|w| w == needle)
}

fn strip_private_modes(bytes: &[u8], strip: &[u32]) -> Vec<u8> {
  // Fast path: if there's no ESC, return as is
  if !bytes.contains(&0x1B) {
    return bytes.to_vec();
  }
  let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] != 0x1B || i + 1 >= bytes.len() || bytes[i + 1] != b'[' {
      out.push(bytes[i]);
      i += 1;
      continue;
    }
    // Potential CSI sequence starts at i (ESC [)
    let start = i;
    let mut j = i + 2;
    // Scan until a final byte in the 0x40..=0x7E range or end
    while j < bytes.len() {
      let ch = bytes[j];
      if (0x40..=0x7E).contains(&ch) {
        break;
      }
      j += 1;
    }
    if j >= bytes.len() {
      // Incomplete CSI - just copy byte and advance one
      out.push(bytes[i]);
      i += 1;
      continue;
    }
    let final_byte = bytes[j];
    // Only sanitize CSI ? ... h/l
    let body = &bytes[i + 2..j];
    let mut handled = false;
    if final_byte == b'h' || final_byte == b'l' {
      // Must contain '?'
      if let Some(pos) = body.iter().position(|&b| b == b'?') {
        // Parse params as ASCII numbers separated by ';'
        let params_bytes = &body[pos + 1..];
        // Gather digits and semicolons only; if any other char exists, bail
        if params_bytes
          .iter()
          .all(|b| b.is_ascii_digit() || *b == b';')
        {
          let s = String::from_utf8_lossy(params_bytes);
          let mut kept: Vec<&str> = Vec::new();
          for part in s.split(';') {
            if part.is_empty() {
              continue;
            }
            // Parse integer; if parse fails, keep conservatively
            match part.parse::<u32>() {
              Ok(n) => {
                if strip.contains(&n) {
                  // strip
                } else {
                  kept.push(part);
                }
              }
              Err(_) => kept.push(part),
            }
          }
          if kept.is_empty() {
            // Drop entire sequence
            handled = true;
          } else {
            // Rewrite CSI ?<kept> final
            out.extend_from_slice(b"\x1b[");
            out.push(b'?');
            let mut first = true;
            for p in kept {
              if !first {
                out.push(b';');
              }
              first = false;
              out.extend_from_slice(p.as_bytes());
            }
            out.push(final_byte);
            handled = true;
          }
        }
      }
    }
    if handled {
      // Skip the original sequence (i..=j)
      i = j + 1;
      continue;
    }
    // Not a handled CSI; copy verbatim
    out.extend_from_slice(&bytes[start..=j]);
    i = j + 1;
  }
  out
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sanitize_transcript_strips_alt_and_modes() {
    let raw = b"hello\x1b[?1049h\x1b[?2004h world\x1b[?1049l!";
    let got = sanitize_transcript(raw);
    assert_eq!(String::from_utf8_lossy(&got), "hello world!");
  }

  #[test]
  fn sanitize_transcript_rewrites_mixed_params() {
    // Keep 25l (cursor hide) but strip 1049
    let raw = b"\x1b[?25;1049lX";
    let got = sanitize_transcript(raw);
    assert_eq!(got, b"\x1b[?25lX");
  }

  #[test]
  fn sanitize_trims_to_last_full_clear() {
    let raw = b"noise\x1b[2Jfresh";
    let got = sanitize_transcript(raw);
    assert_eq!(got, b"fresh");
  }

  #[test]
  fn sanitize_preserves_colors_and_text() {
    let raw = b"\x1b[31mred\x1b[0m and normal";
    let got = sanitize_transcript(raw);
    assert_eq!(got, raw);
  }
}
