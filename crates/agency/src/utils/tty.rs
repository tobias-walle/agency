use std::io::{self, IsTerminal, Read, Write};

use anyhow::{Context, Result};

use crate::log_info;

/// Centralized TTY detection and interactive input handling.
///
/// This struct is initialized once at startup and stored in `AppContext`.
/// Use it for all TTY checks and confirmations to ensure consistent behavior
/// across interactive and non-interactive environments.
#[derive(Debug, Clone)]
pub struct Tty {
  is_interactive: bool,
}

impl Tty {
  /// Create a new Tty instance by checking stdin and stdout.
  #[must_use]
  pub fn new() -> Self {
    let stdin_tty = io::stdin().is_terminal();
    let stdout_tty = io::stdout().is_terminal();
    Self {
      is_interactive: stdin_tty && stdout_tty,
    }
  }

  /// Returns true if both stdin and stdout are connected to a TTY.
  #[must_use]
  pub fn is_interactive(&self) -> bool {
    self.is_interactive
  }

  /// Returns an error if not running in an interactive TTY.
  ///
  /// Use this for commands that absolutely require a TTY (e.g., attach).
  ///
  /// # Errors
  /// Returns an error with a descriptive message if not interactive.
  pub fn require_interactive(&self) -> Result<()> {
    if self.is_interactive {
      Ok(())
    } else {
      anyhow::bail!("This command requires an interactive terminal (TTY)")
    }
  }

  /// Unified confirmation prompt with `-y/--yes` flag support.
  ///
  /// Behavior:
  /// - If `yes_flag` is true: returns `true` immediately (skip prompt)
  /// - If interactive (TTY): shows prompt and waits for user input
  /// - If stdin has piped data (tests): reads from stdin with simple prompt
  /// - If stdin is closed/unavailable: returns `default`
  ///
  /// # Errors
  /// Returns an error if reading from stdin fails.
  pub fn confirm(&self, prompt: &str, default: bool, yes_flag: bool) -> Result<bool> {
    if yes_flag {
      return Ok(true);
    }
    // If interactive TTY, always prompt
    if self.is_interactive {
      return prompt_confirm(prompt, default);
    }
    // Non-interactive: check if stdin has data available (e.g., piped from tests)
    // If stdin is empty or closed, return default without blocking
    if stdin_has_data() {
      return prompt_confirm(prompt, default);
    }
    Ok(default)
  }
}

impl Default for Tty {
  fn default() -> Self {
    Self::new()
  }
}

/// Interactive confirmation prompt that reads from stdin.
fn prompt_confirm(prompt: &str, default: bool) -> Result<bool> {
  let suffix = if default { "[Y/n]" } else { "[y/N]" };
  log_info!("{} {}", prompt, suffix);
  anstream::print!("{}", "-> ".bright_cyan());
  io::stdout().flush().ok();

  let mut input = String::new();
  read_line(&mut input)?;
  let trimmed = input.trim();

  if trimmed.is_empty() {
    return Ok(default);
  }

  let first = trimmed.chars().next().unwrap_or_default();
  Ok(matches!(first, 'y' | 'Y'))
}

/// Check if stdin has data available to read (non-blocking).
/// Returns true if stdin has data, false if empty/closed.
pub fn stdin_has_data() -> bool {
  use std::os::unix::io::AsRawFd;

  let stdin_fd = io::stdin().as_raw_fd();
  let mut poll_fds = [libc::pollfd {
    fd: stdin_fd,
    events: libc::POLLIN,
    revents: 0,
  }];

  // Poll with 0 timeout (non-blocking check)
  let result = unsafe { libc::poll(poll_fds.as_mut_ptr(), 1, 0) };

  // result > 0 means data is available
  result > 0 && (poll_fds[0].revents & libc::POLLIN) != 0
}

/// Read a single line from stdin, byte-by-byte with a size limit.
fn read_line(target: &mut String) -> Result<()> {
  let mut stdin = io::stdin().lock();
  loop {
    let mut buf = [0u8; 1];
    match stdin.read(&mut buf) {
      Ok(0) => break,
      Ok(_) => {
        let ch = buf[0] as char;
        if ch == '\n' || ch == '\r' {
          break;
        }
        target.push(ch);
        if target.len() > 200 {
          break;
        }
      }
      Err(err) => {
        return Err(err).context("failed to read from stdin");
      }
    }
  }
  Ok(())
}

use owo_colors::OwoColorize as _;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn confirm_with_yes_flag_returns_true() {
    let tty = Tty {
      is_interactive: false,
    };
    assert!(tty.confirm("Delete?", false, true).unwrap());
  }

  #[test]
  fn confirm_non_interactive_returns_default() {
    let tty = Tty {
      is_interactive: false,
    };
    assert!(!tty.confirm("Delete?", false, false).unwrap());
    assert!(tty.confirm("Continue?", true, false).unwrap());
  }

  #[test]
  fn require_interactive_fails_when_not_tty() {
    let tty = Tty {
      is_interactive: false,
    };
    let result = tty.require_interactive();
    assert!(result.is_err());
    assert!(result
      .unwrap_err()
      .to_string()
      .contains("interactive terminal"));
  }

  #[test]
  fn require_interactive_succeeds_when_tty() {
    let tty = Tty {
      is_interactive: true,
    };
    assert!(tty.require_interactive().is_ok());
  }
}
