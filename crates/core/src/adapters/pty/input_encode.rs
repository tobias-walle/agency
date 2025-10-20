use crate::rpc::{KeyCodeDTO, KeyCombinationDTO, ModifiersDTO};

pub fn encode_event(ev: &KeyCombinationDTO) -> Vec<u8> {
  let mut out = Vec::new();
  let m = &ev.modifiers;
  match &ev.code {
    KeyCodeDTO::Char(c) => {
      let b = *c as u8;
      if m.ctrl {
        // Legacy C0 control for ctrl-letter if ASCII letter
        let u = (*c as u8).to_ascii_uppercase();
        if (b'A'..=b'Z').contains(&u) {
          out.push(u & 0x1F);
        } else {
          // fall back to original byte
          out.push(b);
        }
      } else {
        out.push(b);
      }
    }
    KeyCodeDTO::Enter => out.push(b'\r'), // CR is typical for Enter
    KeyCodeDTO::Backspace => out.push(0x7F),
    KeyCodeDTO::Tab => out.push(b'\t'),
    KeyCodeDTO::Up => out.extend_from_slice(b"\x1b[A"),
    KeyCodeDTO::Down => out.extend_from_slice(b"\x1b[B"),
    KeyCodeDTO::Left => out.extend_from_slice(b"\x1b[D"),
    KeyCodeDTO::Right => out.extend_from_slice(b"\x1b[C"),
    KeyCodeDTO::Home => out.extend_from_slice(b"\x1b[H"),
    KeyCodeDTO::End => out.extend_from_slice(b"\x1b[F"),
    KeyCodeDTO::PageUp => out.extend_from_slice(b"\x1b[5~"),
    KeyCodeDTO::PageDown => out.extend_from_slice(b"\x1b[6~"),
    KeyCodeDTO::F(n) => {
      // Basic xterm function key encoding: F1..F4 use ESC OP..OS; others ESC [15~ etc.
      match *n {
        1 => out.extend_from_slice(b"\x1bOP"),
        2 => out.extend_from_slice(b"\x1bOQ"),
        3 => out.extend_from_slice(b"\x1bOR"),
        4 => out.extend_from_slice(b"\x1bOS"),
        5 => out.extend_from_slice(b"\x1b[15~"),
        6 => out.extend_from_slice(b"\x1b[17~"),
        7 => out.extend_from_slice(b"\x1b[18~"),
        8 => out.extend_from_slice(b"\x1b[19~"),
        9 => out.extend_from_slice(b"\x1b[20~"),
        10 => out.extend_from_slice(b"\x1b[21~"),
        11 => out.extend_from_slice(b"\x1b[23~"),
        12 => out.extend_from_slice(b"\x1b[24~"),
        _ => {}
      }
    }
  }

  // Apply Alt modifier as ESC prefix for printable keys (common behavior)
  if m.alt {
    // For simple bytes, prefix ESC. For sequences, prefer leaving as-is.
    // Here we only wrap single-byte outputs.
    if out.len() == 1 {
      let b = out[0];
      let mut seq = Vec::with_capacity(2);
      seq.push(0x1B);
      seq.push(b);
      return seq;
    }
  }

  out
}

pub fn encode_events(events: &[KeyCombinationDTO]) -> Vec<u8> {
  let mut out = Vec::new();
  for ev in events {
    out.extend(encode_event(ev));
  }
  out
}
