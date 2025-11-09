/// Token styling helpers.
///
/// The `t` module stands for "tokens". Use these helpers to style
/// specific values inside info messages consistently across the CLI.
pub mod t {
  use std::fmt::Display;

  use owo_colors::OwoColorize as _;
  pub fn id(value: impl Display) -> String {
    format!("{}", value.to_string().blue())
  }

  pub fn path(p: impl Display) -> String {
    format!("{}", p.to_string().cyan())
  }

  pub fn slug(slug: impl Display) -> String {
    format!("{}", slug.to_string().magenta())
  }

  pub fn ok(s: impl Display) -> String {
    format!("{}", s.to_string().green())
  }

  pub fn warn(s: impl Display) -> String {
    format!("{}", s.to_string().yellow())
  }

  #[allow(dead_code)]
  pub fn err(s: impl Display) -> String {
    format!("{}", s.to_string().red())
  }
}

// Lightweight routed logging: when a sink is set, macros emit events to it; otherwise print.
// These macros enforce the agreed style: info = neutral, success/warn/error = full-line tint.
// Use `t::*` helpers to highlight tokens in info messages only.

use crossbeam_channel::Sender;
use parking_lot::Mutex;

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogLevel {
  Info,
  Success,
  Warn,
  Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEvent {
  /// Render a command preface line like "> agency ..."
  Command(String),
  /// A single log line preserving ANSI styling
  Line { level: LogLevel, ansi: String },
}

static SINK: Mutex<Option<Sender<LogEvent>>> = Mutex::new(None);

pub fn set_log_sink(sender: Sender<LogEvent>) {
  *SINK.lock() = Some(sender);
}

pub fn clear_log_sink() {
  *SINK.lock() = None;
}

/// Returns true when a TUI log sink is currently registered
pub fn is_sink_set() -> bool {
  SINK.lock().is_some()
}

pub(crate) fn emit(level: LogLevel, text: String) {
  if let Some(tx) = SINK.lock().clone() {
    // Route into TUI sink
    let _ = tx.send(LogEvent::Line { level, ansi: text });
  } else {
    // Fallback to printing as before
    match level {
      LogLevel::Info => anstream::println!("{}", text),
      LogLevel::Success | LogLevel::Warn => anstream::println!("{}", text),
      LogLevel::Error => anstream::eprintln!("{}", text),
    }
  }
}

#[macro_export]
macro_rules! log_info {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    $crate::utils::log::emit(
      $crate::utils::log::LogLevel::Info,
      format!($fmt $(, $args )*)
    );
  }};
}

#[macro_export]
macro_rules! log_success {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    $crate::utils::log::emit(
      $crate::utils::log::LogLevel::Success,
      $crate::utils::log::t::ok(format!($fmt $(, $args )*))
    );
  }};
}

#[macro_export]
macro_rules! log_warn {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    $crate::utils::log::emit(
      $crate::utils::log::LogLevel::Warn,
      $crate::utils::log::t::warn(format!($fmt $(, $args )*))
    );
  }};
}

#[macro_export]
macro_rules! log_error {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    $crate::utils::log::emit(
      $crate::utils::log::LogLevel::Error,
      $crate::utils::log::t::err(format!($fmt $(, $args )*))
    );
  }};
}

#[cfg(test)]
mod tests {
  use super::*;
  use crossbeam_channel::unbounded;

  #[test]
  fn macros_emit_events_when_sink_set() {
    let (tx, rx) = unbounded();
    set_log_sink(tx);
    // info
    crate::log_info!("Hello {}", "world");
    // success (tinted)
    crate::log_success!("OK {}", 1);
    // warn (tinted)
    crate::log_warn!("Warn {}", 2);
    // error (tinted)
    crate::log_error!("Err {}", 3);

    let evs: Vec<LogEvent> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(evs.len() >= 4);
    // First event should be info line without tinting applied by t:: helpers
    match &evs[0] {
      LogEvent::Line { level, ansi } => {
        assert_eq!(*level, LogLevel::Info);
        assert!(ansi.contains("Hello world"));
      }
      LogEvent::Command(_) => panic!("unexpected event 0"),
    }
    // Success/warn/error should contain ANSI escapes ("\x1b[")
    let mut found_tinted = 0;
    for ev in evs.into_iter().skip(1) {
      if let LogEvent::Line {
        level: LogLevel::Success | LogLevel::Warn | LogLevel::Error,
        ansi,
      } = ev
        && ansi.contains("\u{1b}[")
      {
        found_tinted += 1;
      }
    }
    assert!(found_tinted >= 3);
    clear_log_sink();
  }

  #[test]
  fn macros_no_panic_without_sink() {
    clear_log_sink();
    crate::log_info!("A");
    crate::log_success!("B");
    crate::log_warn!("C");
    crate::log_error!("D");
  }
}
