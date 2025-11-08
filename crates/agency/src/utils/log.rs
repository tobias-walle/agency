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

// Crate-level logging macros using anstream and token helpers.
// These macros enforce the agreed style: info = neutral, success/warn/error = full-line tint.
// Use `t::*` helpers to highlight tokens in info messages only.

#[macro_export]
macro_rules! log_info {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    anstream::println!("{}", format!($fmt $(, $args )*));
  }};
}

#[macro_export]
macro_rules! log_success {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    anstream::println!("{}", $crate::utils::log::t::ok(format!($fmt $(, $args )*)));
  }};
}

#[macro_export]
macro_rules! log_warn {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    anstream::println!("{}", $crate::utils::log::t::warn(format!($fmt $(, $args )*)));
  }};
}

#[macro_export]
macro_rules! log_error {
  ($fmt:literal $(, $args:expr )* $(,)?) => {{
    anstream::eprintln!("{}", $crate::utils::log::t::err(format!($fmt $(, $args )*)));
  }};
}
