use crokey::KeyCombination;

/// Parse a comma-separated list of key combinations into Crokey combinations.
///
/// Returns normalized `KeyCombination`s. Defaults to single `ctrl-q` when
/// nothing valid is parsed.
pub fn parse_detach_keys(s: &str) -> Vec<KeyCombination> {
  let mut combos: Vec<KeyCombination> = Vec::new();

  for part in s.split(',') {
    let raw = part.trim();
    if raw.is_empty() {
      continue;
    }
    match crokey::parse(raw) {
      Ok(kc) => combos.push(kc.normalized()),
      Err(_e) => {}
    }
  }

  if combos.is_empty() {
    combos.push(crokey::key!(ctrl-q));
  }

  combos
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_ctrl_q_on_empty() {
    let v = parse_detach_keys("");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0], crokey::key!(ctrl-q));
  }

  #[test]
  fn parses_single_ctrl_q() {
    let v = parse_detach_keys("ctrl-q");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0], crokey::key!(ctrl-q));
  }

  #[test]
  fn parses_case_insensitive_and_spaces() {
    let v = parse_detach_keys("  Ctrl-P , CTRL-Q  ");
    assert_eq!(v.len(), 2);
    assert_eq!(v[0], crokey::key!(ctrl-p));
    assert_eq!(v[1], crokey::key!(ctrl-q));
  }
}
