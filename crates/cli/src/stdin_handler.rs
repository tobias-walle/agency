use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::{cmp, collections::VecDeque};
use tracing::{debug, trace};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
  pub id: String,
  pub bytes: Vec<u8>,
  pub consume: bool,
}

#[derive(Debug, Default)]
pub struct StdinHandler {
  bindings: Vec<KeyBinding>,
  max_seq_len: usize,
  pending: VecDeque<u8>,
}

impl StdinHandler {
  pub fn new(bindings: Vec<KeyBinding>) -> Self {
    let max_seq_len = bindings.iter().map(|b| b.bytes.len()).max().unwrap_or(0);
    Self {
      bindings,
      max_seq_len,
      pending: VecDeque::new(),
    }
  }

  fn pending_as_slice(&self) -> Vec<u8> {
    self.pending.iter().copied().collect()
  }

  fn longest_match_index(&self, window: &[u8]) -> Option<usize> {
    let mut best_len = 0usize;
    let mut best_idx: Option<usize> = None;
    for (idx, b) in self.bindings.iter().enumerate() {
      let bl = b.bytes.len();
      if bl == 0 || bl > window.len() {
        continue;
      }
      if window[window.len() - bl..] == b.bytes[..] && bl > best_len {
        best_len = bl;
        best_idx = Some(idx);
      }
    }
    if let Some(idx) = best_idx {
      let matched_len = best_len;
      let matched_suffix = &window[window.len() - matched_len..];
      // If a longer binding starts with the matched suffix, defer match
      let longer_exists = self
        .bindings
        .iter()
        .any(|b| b.bytes.len() > matched_len && b.bytes.starts_with(matched_suffix));
      if longer_exists {
        trace!(
          defer_len = matched_len,
          "defer_short_match_for_longer_prefix"
        );
        return None;
      }
      return Some(idx);
    }
    None
  }

  fn longest_prefix_suffix_len(&self, window: &[u8]) -> usize {
    if self.max_seq_len == 0 || window.is_empty() {
      return 0;
    }
    let max_keep = self.max_seq_len.saturating_sub(1);
    let max_keep = cmp::min(max_keep, window.len());
    for k in (1..=max_keep).rev() {
      let suf = &window[window.len() - k..];
      if self.bindings.iter().any(|b| b.bytes.starts_with(suf)) {
        return k;
      }
    }
    0
  }

  pub fn process_chunk(&mut self, chunk: &[u8]) -> (Vec<u8>, Vec<String>) {
    trace!(len = chunk.len(), "stdin_chunk");
    let mut out: Vec<u8> = Vec::with_capacity(chunk.len());
    let mut events: Vec<String> = Vec::new();

    for &byte in chunk {
      self.pending.push_back(byte);
      trace!(
        byte = byte as u32,
        pending_len = self.pending.len(),
        "push_byte"
      );

      loop {
        let win = self.pending_as_slice();
        if let Some(idx) = self.longest_match_index(&win) {
          let b = &self.bindings[idx];
          let bl = b.bytes.len();
          // remove matched portion from pending tail
          for _ in 0..bl {
            self.pending.pop_back();
          }
          if !b.consume {
            out.extend_from_slice(&b.bytes);
          }
          trace!(id = %b.id, len = bl, consume = b.consume, "binding_match");
          events.push(b.id.clone());

          // After a match, retain only the longest suffix that could start a binding
          let win2 = self.pending_as_slice();
          let keep = self.longest_prefix_suffix_len(&win2);
          while self.pending.len() > keep {
            if let Some(x) = self.pending.pop_front() {
              out.push(x);
            }
          }
          // continue loop to catch cascading matches
          continue;
        }
        break;
      }

      // Enforce withholding bounds progressively
      let win = self.pending_as_slice();
      let keep = self.longest_prefix_suffix_len(&win);
      while self.pending.len() > keep {
        if let Some(x) = self.pending.pop_front() {
          out.push(x);
        }
      }
    }

    // End-of-chunk: flush anything that is not part of a possible prefix
    let win = self.pending_as_slice();
    let keep = self.longest_prefix_suffix_len(&win);
    let mut flushed = 0usize;
    while self.pending.len() > keep {
      if let Some(x) = self.pending.pop_front() {
        out.push(x);
        flushed += 1;
      }
    }
    debug!(
      out_len = out.len(),
      events = events.len(),
      flushed,
      keep,
      "stdin_chunk_processed"
    );
    (out, events)
  }

  pub fn flush_pending(&mut self) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(b) = self.pending.pop_front() {
      out.push(b);
    }
    out
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
  Data(Vec<u8>),
  Binding(String),
}

pub fn spawn_stdin_reader(
  bindings: Vec<KeyBinding>,
  tx: mpsc::Sender<Msg>,
) -> thread::JoinHandle<()> {
  thread::spawn(move || {
    let mut stdin = std::io::stdin();
    let mut buf = [0u8; 1024];
    let mut handler = StdinHandler::new(bindings);
    loop {
      match stdin.read(&mut buf) {
        Ok(0) => {
          let flushed = handler.flush_pending();
          if !flushed.is_empty() {
            let _ = tx.send(Msg::Data(flushed));
          }
          break;
        }
        Ok(n) => {
          trace!(n, "stdin_read");
          let (data, bindings) = handler.process_chunk(&buf[..n]);
          if !data.is_empty() {
            let _ = tx.send(Msg::Data(data));
          }
          for id in bindings {
            let _ = tx.send(Msg::Binding(id));
          }
        }
        Err(e) => {
          debug!(error = %e, "stdin_read_error");
          break;
        }
      }
    }
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn kb(id: &str, bytes: &[u8], consume: bool) -> KeyBinding {
    KeyBinding {
      id: id.to_string(),
      bytes: bytes.to_vec(),
      consume,
    }
  }

  #[test]
  fn single_key_consumed_with_noise() {
    let mut h = StdinHandler::new(vec![kb("detach", &[0x11], true)]);
    let (p1, e1) = h.process_chunk(b"ab");
    assert_eq!(p1, b"ab");
    assert!(e1.is_empty());

    let (p2, e2) = h.process_chunk(&[0x11]);
    assert!(p2.is_empty(), "should have consumed detach");
    assert!(
      e2.contains(&"detach".to_string()),
      "should emit detach event"
    );
  }

  #[test]
  fn multi_key_across_chunks() {
    let mut h = StdinHandler::new(vec![kb("detach", &[0x10, 0x11], true)]);
    let (_p1, e1) = h.process_chunk(&[0x10]);
    assert!(e1.is_empty());
    let (p2, e2) = h.process_chunk(&[0x11]);
    assert!(p2.is_empty(), "consume matched bytes");
    assert!(e2.contains(&"detach".to_string()));
  }

  #[test]
  fn overlapping_longest_match() {
    let mut h = StdinHandler::new(vec![
      kb("short", &[0x10], true),
      kb("long", &[0x10, 0x11], true),
    ]);
    let (p, e) = h.process_chunk(&[0x10, 0x11]);
    assert!(p.is_empty());
    assert!(e.contains(&"long".to_string()));
    assert!(!e.contains(&"short".to_string()));
  }

  #[test]
  fn partial_end_of_chunk_flush() {
    let mut h = StdinHandler::new(vec![kb("x", b"ab", true)]);
    let (p1, _e1) = h.process_chunk(b"ac");
    // 'a' should be flushed because 'ac' does not prefix any binding
    assert_eq!(String::from_utf8_lossy(&p1), "ac");
  }

  #[test]
  fn eof_flush_returns_remaining_pending() {
    let mut h = StdinHandler::new(vec![kb("x", b"ab", true)]);
    let (_p, _e) = h.process_chunk(b"a");
    let f = h.flush_pending();
    assert_eq!(f, vec![b'a']);
  }
}
