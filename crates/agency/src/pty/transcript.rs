use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

/// Capped transcript of raw PTY output, stored as byte chunks.
///
/// Maintains a FIFO queue of chunks and enforces a maximum total byte cap
/// by evicting from the front. Thread-safe with short lock scopes.
pub struct Transcript {
  cap_bytes: u64,
  chunks: Mutex<VecDeque<Vec<u8>>>,
  total: AtomicU64,
}

impl Transcript {
  /// Create a new transcript with the given byte cap.
  #[must_use]
  pub fn new(cap_bytes: u64) -> Self {
    Self {
      cap_bytes,
      chunks: Mutex::new(VecDeque::new()),
      total: AtomicU64::new(0),
    }
  }

  /// Append a new chunk and evict from the front until under cap.
  pub fn push(&self, bytes: &[u8]) {
    let mut q = self.chunks.lock();
    q.push_back(bytes.to_vec());
    let prev = self.total.fetch_add(bytes.len() as u64, Ordering::Relaxed);
    let mut total = prev + bytes.len() as u64;
    while total > self.cap_bytes {
      if let Some(front) = q.pop_front() {
        let len = front.len() as u64;
        total = total.saturating_sub(len);
        self.total.fetch_sub(len, Ordering::Relaxed);
      } else {
        break;
      }
    }
  }

  /// Clear all chunks and reset total.
  pub fn clear(&self) {
    let mut q = self.chunks.lock();
    q.clear();
    self.total.store(0, Ordering::Relaxed);
  }

  /// Gather all chunks into a contiguous Vec.
  #[must_use]
  pub fn gather(&self) -> Vec<u8> {
    let total = self.total.load(Ordering::Relaxed) as usize;
    let mut out = Vec::with_capacity(total);
    let q = self.chunks.lock();
    for c in q.iter() {
      out.extend_from_slice(c);
    }
    out
  }

  /// Current total size in bytes.
  #[must_use]
  pub fn total(&self) -> u64 {
    self.total.load(Ordering::Relaxed)
  }
}
