//! Terminal reset footer emitted on leaving interactive attach.
//!
//! Writes a small, safe, idempotent set of control sequences to
//! restore common terminal modes and dynamic colors.
//! Guard emission behind a TTY check at call site.
//!
//! References: XTerm Control Sequences
//! - Alt-screen leave: CSI ? 1049/1047/47 l
//!   (soft reset omitted; some terminals echo mode reports)
//! - Show cursor: CSI ? 25 h
//! - Reset SGR: CSI 0 m
//! - Bracketed paste off: CSI ? 2004 l
//! - Mouse modes off: ?1000/1002/1003/1005/1006/1015/1016 l
//! - Dynamic color reset: OSC 110/111/112 ST

use std::io::Write;

#[allow(dead_code)]
const ESC: u8 = 0x1B; // '\x1b'
#[allow(dead_code)]
const CSI: &[u8] = b"["; // Control Sequence Introducer after ESC
#[allow(dead_code)]
const OSC: &[u8] = b"]"; // Operating System Command after ESC
const ST: &[u8] = &[0x1B, b'\\']; // String Terminator (ESC \\)

// Core reset sequences
const LEAVE_ALT_1049: &[u8] = b"\x1b[?1049l";
const LEAVE_ALT_1047: &[u8] = b"\x1b[?1047l";
const LEAVE_ALT_47: &[u8] = b"\x1b[?47l";
const SHOW_CURSOR: &[u8] = b"\x1b[?25h";
const SGR_RESET: &[u8] = b"\x1b[0m";

// Input modes off
const BRACKETED_PASTE_OFF: &[u8] = b"\x1b[?2004l";
const MOUSE_1000_OFF: &[u8] = b"\x1b[?1000l";
const MOUSE_1002_OFF: &[u8] = b"\x1b[?1002l";
const MOUSE_1003_OFF: &[u8] = b"\x1b[?1003l";
const MOUSE_1005_OFF: &[u8] = b"\x1b[?1005l";
const MOUSE_1006_OFF: &[u8] = b"\x1b[?1006l";
const MOUSE_1015_OFF: &[u8] = b"\x1b[?1015l";
const MOUSE_1016_OFF: &[u8] = b"\x1b[?1016l";

// Dynamic color resets (OSC ... ST)
const OSC_RESET_FG: &[u8] = b"\x1b]110"; // + ST
const OSC_RESET_BG: &[u8] = b"\x1b]111"; // + ST
const OSC_RESET_CURSOR: &[u8] = b"\x1b]112"; // + ST

// Optional: restore default cursor style
const CURSOR_STYLE_DEFAULT: &[u8] = b"\x1b[0q";

/// Write the terminal reset footer to `w` in a safe order.
///
/// Excludes DECSTR (soft reset) to avoid triggering terminal response
/// sequences (e.g. DECRPM), which would echo escape codes into stdout on
/// some terminals when the user detaches.
pub fn write_reset_footer<W: Write>(w: &mut W) -> std::io::Result<()> {
  // Leave alt-screen modes
  w.write_all(LEAVE_ALT_1049)?;
  w.write_all(LEAVE_ALT_1047)?;
  w.write_all(LEAVE_ALT_47)?;

  // Visual resets
  w.write_all(SHOW_CURSOR)?;
  w.write_all(SGR_RESET)?;

  // Input modes off
  w.write_all(BRACKETED_PASTE_OFF)?;
  w.write_all(MOUSE_1000_OFF)?;
  w.write_all(MOUSE_1002_OFF)?;
  w.write_all(MOUSE_1003_OFF)?;
  w.write_all(MOUSE_1005_OFF)?;
  w.write_all(MOUSE_1006_OFF)?;
  w.write_all(MOUSE_1015_OFF)?;
  w.write_all(MOUSE_1016_OFF)?;

  // Dynamic color resets with ST terminator
  w.write_all(OSC_RESET_FG)?;
  w.write_all(ST)?;
  w.write_all(OSC_RESET_BG)?;
  w.write_all(ST)?;
  w.write_all(OSC_RESET_CURSOR)?;
  w.write_all(ST)?;

  // Optional cursor style default
  w.write_all(CURSOR_STYLE_DEFAULT)?;

  w.flush()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reset_footer_excludes_soft_reset() {
    let mut buf = Vec::new();
    write_reset_footer(&mut buf).expect("write reset footer");
    assert!(
      !buf.windows(4).any(|w| w == b"\x1b[!p"),
      "should not emit DECSTR"
    );
    assert!(
      buf
        .windows(LEAVE_ALT_1049.len())
        .any(|w| w == LEAVE_ALT_1049),
      "should leave alt screen"
    );
  }
}
