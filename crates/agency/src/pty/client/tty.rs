//! Terminal helpers used by the attach client.
//!
//! These helpers explain why raw mode is used.
//! The client reads stdin and detects a literal Ctrl-Q byte to
//! translate it into a protocol `Detach` message rather than sending SIGINT to
//! the client process itself.

use anyhow::Result;
use crossterm::terminal;

/// Disables raw terminal mode for the lifetime of this guard and
/// re-enables it when dropped.
///
/// This is used for printing human-readable logs (like session stats)
/// where normal newline handling (CRLF) is desired. In raw mode, the
/// terminal typically does not translate `\n` into `\r\n`, which causes
/// misaligned output when mixing logs with PTY bytes. Pausing raw mode
/// for the short duration of logging restores expected rendering.
pub struct RawModePauseGuard;
impl RawModePauseGuard {
  pub fn pause() -> Self {
    // Best-effort: ignore errors during toggling; the original
    // `RawModeGuard` remains the source of truth for attach lifetime.
    let _ = terminal::disable_raw_mode();
    Self
  }
}
impl Drop for RawModePauseGuard {
  fn drop(&mut self) {
    let _ = terminal::enable_raw_mode();
  }
}

/// Enables raw terminal mode for the lifetime of this guard.
pub struct RawModeGuard;
impl RawModeGuard {
  pub fn enable() -> Result<Self> {
    terminal::enable_raw_mode()?;
    Ok(Self)
  }
}
impl Drop for RawModeGuard {
  fn drop(&mut self) {
    let _ = terminal::disable_raw_mode();
  }
}
