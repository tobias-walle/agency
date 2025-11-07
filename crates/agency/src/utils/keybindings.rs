use crate::config::AgencyConfig;
use anyhow::{Result, bail};
use termwiz::input::{KeyCode, Modifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keybinding {
  pub mods: Modifiers,
  pub key: KeyCode,
}

impl Keybinding {
  pub fn matches(&self, event_mods: Modifiers, event_key: &KeyCode) -> bool {
    self.mods == event_mods && &self.key == event_key
  }
}

fn parse_modifier(token: &str) -> Option<Modifiers> {
  match token {
    "ctrl" | "CTRL" => Some(Modifiers::CTRL),
    "alt" | "ALT" => Some(Modifiers::ALT),
    "shift" | "SHIFT" => Some(Modifiers::SHIFT),
    "super" | "SUPER" => Some(Modifiers::SUPER),
    _ => None,
  }
}

/// Parse a case-sensitive binding string like "ctrl-q" or "Ä" into a Keybinding.
///
/// Rules:
/// - Format: tokens separated by '-', last token is the key.
/// - Accepted modifiers: ctrl, alt, shift, super (also uppercase variants).
/// - Key: single Unicode scalar value (one character).
/// - Uppercase single letters (including locale, e.g., 'Ä') are normalized by adding SHIFT
///   and using the uppercase character as the `KeyCode::Char`.
/// - Strict: whitespace and unknown tokens cause failure.
pub fn parse_binding_str(s: &str) -> Result<Keybinding> {
  if s.is_empty() {
    bail!("empty keybinding string")
  }
  if s.chars().any(char::is_whitespace) {
    bail!("whitespace not allowed in keybinding: '{s}'")
  }

  let parts: Vec<&str> = s.split('-').collect();
  let mut mods = Modifiers::NONE;
  for tok in &parts[..parts.len().saturating_sub(1)] {
    let Some(m) = parse_modifier(tok) else {
      bail!("invalid modifier '{tok}' in '{s}'")
    };
    mods |= m;
  }

  let key_tok = parts.last().unwrap();
  match key_tok.chars().next() {
    None => bail!("missing key in '{s}'"),
    Some(ch) => {
      if key_tok.chars().count() != 1 {
        bail!("key must be a single character in '{s}'")
      }
      if ch.is_uppercase() {
        mods |= Modifiers::SHIFT;
      }
      Ok(Keybinding {
        mods,
        key: KeyCode::Char(ch),
      })
    }
  }
}

/// Parse the detach keybinding from merged config, failing fast if missing or invalid.
pub fn parse_detach_key(cfg: &AgencyConfig) -> Result<Keybinding> {
  let Some(kb_cfg) = cfg.keybindings.as_ref() else {
    bail!("missing [keybindings] section in config")
  };
  if kb_cfg.detach.trim().is_empty() {
    bail!("keybindings.detach must not be empty")
  }
  parse_binding_str(&kb_cfg.detach)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_ctrl_q() {
    let kb = parse_binding_str("ctrl-q").unwrap();
    assert_eq!(kb.mods, Modifiers::CTRL);
    assert_eq!(kb.key, KeyCode::Char('q'));
  }

  #[test]
  fn rejects_uppercase_modifier_only() {
    let kb = parse_binding_str("CTRL-q").unwrap();
    assert_eq!(kb.mods, Modifiers::CTRL);
    assert_eq!(kb.key, KeyCode::Char('q'));
  }

  #[test]
  fn normalizes_uppercase_ascii_letter() {
    let kb = parse_binding_str("Q").unwrap();
    assert!(kb.mods.contains(Modifiers::SHIFT));
    assert_eq!(kb.key, KeyCode::Char('Q'));
  }

  #[test]
  fn supports_locale_uppercase_letter() {
    let kb = parse_binding_str("Ä").unwrap();
    assert!(kb.mods.contains(Modifiers::SHIFT));
    assert_eq!(kb.key, KeyCode::Char('Ä'));
  }

  #[test]
  fn rejects_multi_char_key() {
    let err = parse_binding_str("ctrl-qq").unwrap_err();
    assert!(format!("{err}").contains("single character"));
  }

  #[test]
  fn rejects_unknown_modifier() {
    let err = parse_binding_str("meta-q").unwrap_err();
    assert!(format!("{err}").contains("invalid modifier"));
  }

  #[test]
  fn rejects_whitespace() {
    let err = parse_binding_str("ctrl - q").unwrap_err();
    assert!(format!("{err}").contains("whitespace"));
  }
}
