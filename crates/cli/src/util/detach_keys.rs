pub fn parse_detach_keys(s: &str) -> Vec<u8> {
  let mut seq = Vec::new();
  for part in s.split(',') {
    let p = part.trim().to_ascii_lowercase();
    if let Some(rest) = p.strip_prefix("ctrl-")
      && let Some(ch) = rest.chars().next()
    {
      let upper = ch.to_ascii_uppercase();
      let code = (upper as u8) & 0x1f;
      seq.push(code);
    }
  }
  if seq.is_empty() {
    seq.push((b'Q') & 0x1f);
  }
  seq
}
