fn is_printable_ascii(b: u8) -> bool {
  b >= 0x20 && b != 0x7F
}

pub(crate) fn sanitize_with_counters(input: &[u8]) -> (Vec<u8>, usize, usize) {
  if input.is_empty() {
    return (Vec::new(), 0, 0);
  }
  let mut dropped_head = 0usize;
  let mut dropped_tail = 0usize;

  let mut start = 0usize;
  let first = input[0];
  let at_safe = first == b'\n' || first == 0x1B || is_printable_ascii(first);
  if !at_safe && let Some(pos) = input.iter().position(|&b| b == b'\n') {
    start = pos.saturating_add(1);
  }

  while start < input.len() {
    let b = input[start];
    if b == b'\n' || b == 0x1B || is_printable_ascii(b) {
      if b == b'[' {
        let mut i = start + 1;
        let mut _found_final = false;
        while i < input.len() {
          let bb = input[i];
          if (0x40..=0x7E).contains(&bb) {
            _found_final = true;
            i += 1;
            break;
          }
          i += 1;
        }
        dropped_head += i - start;
        start = i;
        continue;
      }
      break;
    }
    start += 1;
    dropped_head += 1;
  }

  let mut out = Vec::with_capacity(input.len().saturating_sub(start));
  let mut i = start;
  while i < input.len() {
    let b = input[i];
    if b == b'\r' {
      if i + 1 < input.len() && input[i + 1] == b'\n' {
        // Normalize CRLF to LF
        out.push(b'\n');
        i += 2;
        continue;
      } else {
        // Normalize lone CR to LF
        out.push(b'\n');
        i += 1;
        continue;
      }
    }
    out.push(b);
    i += 1;
  }

  if let Some(last_esc_pos) = out.iter().rposition(|&b| b == 0x1B) {
    if last_esc_pos == out.len() - 1 {
      out.truncate(last_esc_pos);
      dropped_tail += 1;
    } else if out.get(last_esc_pos + 1) == Some(&b'[') {
      let mut j = last_esc_pos + 2;
      let mut has_final = false;
      while j < out.len() {
        let bb = out[j];
        if (0x40..=0x7E).contains(&bb) {
          has_final = true;
          break;
        }
        j += 1;
      }
      if !has_final {
        dropped_tail += out.len() - last_esc_pos;
        out.truncate(last_esc_pos);
      }
    }
  }

  (out, dropped_head, dropped_tail)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn sanitize_replay(input: &[u8]) -> Vec<u8> {
  sanitize_with_counters(input).0
}

#[cfg(test)]
mod tests {
  use super::*;

  fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
  }

  #[test]
  fn sanitize_drops_mid_csi_head_and_keeps_plain_text() {
    let input = bytes("[31mHello");
    let out = sanitize_replay(&input);
    assert_eq!(String::from_utf8_lossy(&out), "Hello");
  }

  #[test]
  fn sanitize_truncates_dangling_escape_tails() {
    let a = bytes("Hello\x1b");
    let b = bytes("Hello\x1b[");
    let c = bytes("Hello\x1b[31");
    assert_eq!(String::from_utf8_lossy(&sanitize_replay(&a)), "Hello");
    assert_eq!(String::from_utf8_lossy(&sanitize_replay(&b)), "Hello");
    assert_eq!(String::from_utf8_lossy(&sanitize_replay(&c)), "Hello");
  }

  #[test]
  fn sanitize_converts_isolated_cr_to_lf() {
    let input = bytes("progress 1\rprogress 2\rprogress 3\n");
    let out = sanitize_replay(&input);
    assert_eq!(
      String::from_utf8_lossy(&out),
      "progress 1\nprogress 2\nprogress 3\n"
    );
  }

  #[test]
  fn sanitize_preserves_complete_ansi_sequences() {
    let input = bytes("\x1b[31mHello\x1b[0m\n");
    let out = sanitize_replay(&input);
    assert_eq!(out, input);
  }
}
