//! Terminal reset footer emitted on leaving interactive attach.
//!
//! Writes a small, safe, idempotent set of crossterm commands to
//! restore common terminal modes without depending on terminal state.
//! Guard emission behind a TTY check at call site.
//!
//! Goals:
//! - Stateless and cross-terminal safe
//! - Leave alternate screen
//! - Show cursor
//! - Reset SGR colors
//! - Re-enable line wrapping
//! - Disable bracketed paste if enabled
//
use std::io::Write;

use crossterm::{
  cursor::Show,
  event::DisableBracketedPaste,
  execute,
  style::ResetColor,
  terminal::{EnableLineWrap, LeaveAlternateScreen},
};

/// Write a stateless terminal reset footer to `w`.
///
/// This avoids DECSTR and other stateful soft resets that may trigger
/// terminal responses. It only emits commands that are broadly supported
/// and safe to call unconditionally.
pub fn write_reset_footer<W: Write>(w: &mut W) -> std::io::Result<()> {
  // Use crossterm commands for safety and portability.
  execute!(
    w,
    LeaveAlternateScreen,
    Show,
    ResetColor,
    EnableLineWrap,
    DisableBracketedPaste
  )?;
  w.flush()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reset_footer_emits_alt_leave_and_no_soft_reset() {
    let mut buf = Vec::new();
    write_reset_footer(&mut buf).expect("write reset footer");
    // Should include alt-screen leave (commonly ESC[?1049l)
    assert!(
      buf.windows(8).any(|w| w == b"\x1b[?1049l"),
      "should leave alt screen"
    );
    // Should not include DECSTR (ESC[!p)
    assert!(
      !buf.windows(4).any(|w| w == b"\x1b[!p"),
      "should not emit DECSTR"
    );
  }
}
