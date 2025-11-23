use std::fmt;
use std::io::{self, IsTerminal as _, Read, Write};

use anyhow::{Context, Result, anyhow};
use inquire::{Confirm, Select, Text};
use owo_colors::OwoColorize as _;

use crate::{log_info, log_warn};

/// ASCII art logo rendered during setup flows.
#[allow(unknown_lints)]
#[allow(clippy::unneeded_raw_string)]
const LOGO_LINES: [&str; 10] = [
  r"       db",
  r"      d88b",
  r"     d8'`8b",
  r"    d8'  `8b      ,adPPYb,d8   ,adPPYba,  8b,dPPYba,    ,adPPYba, `8b        d8'",
  r#"   d8YaaaaY8b    a8"    `Y88  a8P_____88  88P'   "8a,  a8"     ""  `8b      d8'"#,
  r#"  d8""""""""8b   8b       88  8PP"""""""  88       88  8b            `8b   d8'"#,
  r#" d8'        `8b  "8a,   ,d88  "8b,   ,aa  88       88  "8a,   ,aa     `8b,d8'"#,
  r#"d8'          `8b  `"YbbdP"Y8   `"Ybbd8"'  88       88   `"Ybbd8"'       d8'"#,
  r"                  aa,    ,88                                            d8'",
  r#"                   "Y8bbdP"                                            d8'"#,
];

/// Choice item that renders nicely in interactive lists while retaining an
/// associated value.
#[derive(Clone, Debug)]
pub struct Choice {
  pub value: String,
  pub label: String,
  pub detail: Option<String>,
}

impl Choice {}

impl fmt::Display for Choice {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.detail {
      Some(detail) => write!(f, "{} {}", self.label.cyan().bold(), detail.dimmed()),
      None => write!(f, "{}", self.label.cyan().bold()),
    }
  }
}

/// Shared helpers for interactive setup wizards.
#[derive(Clone, Debug)]
pub struct Wizard {
  is_tty: bool,
}

impl Wizard {
  #[must_use]
  pub fn new() -> Self {
    let stdin_tty = io::stdin().is_terminal();
    let stdout_tty = io::stdout().is_terminal();
    let interactive = stdin_tty && stdout_tty;
    Self {
      is_tty: interactive,
    }
  }

  /// Print the branded logo using a cyan-to-violet gradient.
  #[allow(clippy::cast_precision_loss)]
  pub fn print_logo() {
    let steps = (LOGO_LINES.len().saturating_sub(1)).max(1) as f32;
    for (idx, line) in LOGO_LINES.iter().enumerate() {
      let (r, g, b) = gradient_color(idx as f32 / steps);
      anstream::println!("{}", line.truecolor(r, g, b));
    }
  }

  /// Print informational lines through the shared logger.
  pub fn info_lines(lines: &[String]) {
    for line in lines {
      if line.is_empty() {
        log_info!("");
      } else {
        log_info!("{}", line);
      }
    }
  }

  /// Select a choice either via `inquire` when attached to a TTY or via a simple
  /// textual fallback when running non-interactively (e.g. tests or piped input).
  pub fn select(
    &self,
    prompt: &str,
    options: &[Choice],
    default_value: Option<&str>,
  ) -> Result<String> {
    if options.is_empty() {
      anyhow::bail!("cannot prompt for selection without any options");
    }
    let default_idx = default_value
      .and_then(|value| options.iter().position(|opt| opt.value == value))
      .unwrap_or(0);
    if self.is_tty {
      return Select::new(prompt, options.to_vec())
        .with_starting_cursor(default_idx)
        .prompt()
        .map(|choice| choice.value)
        .map_err(|err| anyhow!(err));
    }
    Self::fallback_select(prompt, options, default_value)
  }

  /// Prompt for textual input with a default value and trimming applied.
  pub fn text(&self, prompt: &str, default: &str) -> Result<String> {
    if self.is_tty {
      return Text::new(prompt)
        .with_default(default)
        .prompt()
        .map(|ans| ans.trim().to_string())
        .map_err(|err| anyhow!(err));
    }
    Self::fallback_text(prompt, default)
  }

  /// Prompt for a command and split it into argv using shell-words.
  pub fn shell_words(&self, prompt: &str, default_argv: &[String]) -> Result<Vec<String>> {
    let default_str = default_argv.join(" ");
    let input = self.text(prompt, &default_str)?;
    let parts = shell_words::split(&input).context("invalid shell command")?;
    Ok(parts)
  }

  /// Prompt for a yes/no confirmation.
  pub fn confirm(&self, prompt: &str, default: bool) -> Result<bool> {
    if self.is_tty {
      return Confirm::new(prompt)
        .with_default(default)
        .prompt()
        .map_err(|err| anyhow!(err));
    }
    Self::fallback_confirm(prompt, default)
  }

  fn fallback_select(
    prompt: &str,
    options: &[Choice],
    default_value: Option<&str>,
  ) -> Result<String> {
    log_info!("{}", prompt);
    for (idx, opt) in options.iter().enumerate() {
      match &opt.detail {
        Some(detail) => log_info!("  {}. {} {}", idx + 1, opt.label, detail),
        None => log_info!("  {}. {}", idx + 1, opt.label),
      }
    }
    if let Some(def) = default_value {
      log_info!("  (Press Enter to keep {})", def);
    }
    anstream::print!("{}", "-> ".bright_cyan());
    io::stdout().flush().ok();

    let mut input = String::new();
    read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
      if let Some(def) = default_value
        && let Some(choice) = options.iter().find(|opt| opt.value == def)
      {
        return Ok(choice.value.clone());
      }
      return Ok(options[0].value.clone());
    }
    if let Ok(idx) = trimmed.parse::<usize>()
      && (1..=options.len()).contains(&idx)
    {
      return Ok(options[idx - 1].value.clone());
    }
    if let Some(found) = options.iter().find(|opt| {
      opt.value.eq_ignore_ascii_case(trimmed) || opt.label.eq_ignore_ascii_case(trimmed)
    }) {
      return Ok(found.value.clone());
    }
    log_warn!("Invalid selection: {}", trimmed);
    anyhow::bail!("invalid selection")
  }

  fn fallback_text(prompt: &str, default: &str) -> Result<String> {
    log_info!("{} [{}]", prompt, default);
    anstream::print!("{}", "-> ".bright_cyan());
    io::stdout().flush().ok();

    let mut input = String::new();
    read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
      return Ok(default.to_string());
    }
    Ok(trimmed.to_string())
  }

  fn fallback_confirm(prompt: &str, default: bool) -> Result<bool> {
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
}

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

#[allow(
  clippy::cast_precision_loss,
  clippy::cast_possible_truncation,
  clippy::cast_sign_loss
)]
fn gradient_color(t: f32) -> (u8, u8, u8) {
  let start = (0x5e as f32, 0xea as f32, 0xd5 as f32);
  let end = (0xa8 as f32, 0x55 as f32, 0xf7 as f32);
  let ratio = t.clamp(0.0, 1.0);
  let lerp = |a: f32, b: f32| a + (b - a) * ratio;
  (
    lerp(start.0, end.0).round() as u8,
    lerp(start.1, end.1).round() as u8,
    lerp(start.2, end.2).round() as u8,
  )
}
