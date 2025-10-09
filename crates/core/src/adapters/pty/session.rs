use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use tracing::{debug, info};

pub(crate) const MAX_BUFFER_BYTES: usize = 1024 * 1024; // ~1 MiB cap for history ring
pub(crate) const ATTACH_REPLAY_BYTES: usize = 128 * 1024; // 128 KiB replay limit
pub(crate) const ATTACH_REPLAY_EMIT_BYTES: usize = 8 * 1024; // Emit up to 8 KiB on initial prefill
pub(crate) const ALT_TAIL_MAX: usize = 8; // lookbehind window for alt-screen detection

#[derive(Debug, Default, Clone)]
pub(crate) struct AltDetectState {
  pub(crate) active: bool,
  pub(crate) tail: Vec<u8>,
}

pub(crate) fn alt_detect_process(state: &mut AltDetectState, data: &[u8]) {
  let mut scan = Vec::with_capacity(state.tail.len() + data.len());
  scan.extend_from_slice(&state.tail);
  scan.extend_from_slice(data);

  let mut i = 0usize;
  while i + 3 <= scan.len() {
    if scan[i] == 0x1B && i + 2 < scan.len() && scan[i + 1] == b'[' && scan[i + 2] == b'?' {
      let mut j = i + 3;
      while j < scan.len() && scan[j].is_ascii_digit() {
        j += 1;
      }
      if j < scan.len() {
        let num = std::str::from_utf8(&scan[i + 3..j])
          .ok()
          .and_then(|s| s.parse::<u32>().ok());
        let final_byte = scan[j];
        if let Some(n) = num
          && (n == 1049 || n == 1047 || n == 47)
          && (final_byte == b'h' || final_byte == b'l')
        {
          let before = state.active;
          state.active = final_byte == b'h';
          if state.active != before {
            if state.active {
              info!(event = "pty_alt_screen_on", num, "entered alt screen");
            } else {
              info!(event = "pty_alt_screen_off", num, "left alt screen");
            }
          }
          i = j + 1;
          continue;
        }
      }
    }
    i += 1;
  }

  if scan.len() > ALT_TAIL_MAX {
    state.tail = scan[scan.len() - ALT_TAIL_MAX..].to_vec();
  } else {
    state.tail = scan;
  }
}

pub(crate) struct PtySession {
  pub(crate) id: u64,
  pub(crate) master: Mutex<Box<dyn portable_pty::MasterPty + Send>>,
  pub(crate) writer: Mutex<Option<Box<dyn Write + Send>>>,
  pub(crate) history_ring: Mutex<Vec<u8>>,
  pub(crate) outbox: Mutex<Option<Vec<u8>>>,
  pub(crate) eof: AtomicBool,
  pub(crate) active_attach: Mutex<Option<String>>,
  #[allow(dead_code)]
  pub(crate) child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
  pub(crate) cv: (Mutex<bool>, std::sync::Condvar),
  pub(crate) alt_screen_active: AtomicBool,
  pub(crate) alt_detect_tail: Mutex<Vec<u8>>,
}

impl PtySession {
  pub(crate) fn new(
    id: u64,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
  ) -> Self {
    Self {
      id,
      master: Mutex::new(master),
      writer: Mutex::new(None),
      history_ring: Mutex::new(Vec::new()),
      outbox: Mutex::new(None),
      eof: AtomicBool::new(false),
      active_attach: Mutex::new(None),
      child: Mutex::new(Some(child)),
      cv: (Mutex::new(false), std::sync::Condvar::new()),
      alt_screen_active: AtomicBool::new(false),
      alt_detect_tail: Mutex::new(Vec::new()),
    }
  }
}

pub(crate) fn spawn_reader_thread(session: Arc<PtySession>) {
  thread::spawn(move || {
    let reader_res = {
      let master = session.master.lock().unwrap();
      master.try_clone_reader()
    };
    let mut tmp = [0u8; 8192];
    if let Ok(mut reader) = reader_res {
      loop {
        match reader.read(&mut tmp) {
          Ok(0) => {
            session.eof.store(true, Ordering::SeqCst);
            debug!(event = "pty_reader_eof", task_id = session.id, "PTY reader reached EOF");
            break;
          }
          Ok(n) => {
            debug!(event = "pty_reader_read", task_id = session.id, bytes = n);
            let data = &tmp[..n];

            {
              let before = session.alt_screen_active.load(Ordering::SeqCst);
              let mut state = AltDetectState {
                active: before,
                tail: session.alt_detect_tail.lock().unwrap().clone(),
              };
              alt_detect_process(&mut state, data);
              session
                .alt_screen_active
                .store(state.active, Ordering::SeqCst);
              let mut tail = session.alt_detect_tail.lock().unwrap();
              *tail = state.tail;
            }

            {
              let mut ring = session.history_ring.lock().unwrap();
              ring.extend_from_slice(data);
              if ring.len() > MAX_BUFFER_BYTES {
                let excess = ring.len() - MAX_BUFFER_BYTES;
                ring.drain(0..excess);
              }
            }

            {
              let mut outbox_opt = session.outbox.lock().unwrap();
              if let Some(ref mut outbox) = *outbox_opt {
                outbox.extend_from_slice(data);
              }
            }

            {
              let (ref changed_lock, ref cv) = session.cv;
              let mut changed = changed_lock.lock().unwrap();
              *changed = true;
              cv.notify_all();
            }
          }
          Err(e) => {
            session.eof.store(true, Ordering::SeqCst);
            debug!(event = "pty_reader_error", task_id = session.id, error = %e);
            let (ref changed_lock, ref cv) = session.cv;
            let mut changed = changed_lock.lock().unwrap();
            *changed = true;
            cv.notify_all();
            break;
          }
        }
      }
    } else {
      session.eof.store(true, Ordering::SeqCst);
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn bytes(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
  }

  #[test]
  fn alt_screen_detection_enters_and_leaves() {
    let mut st = AltDetectState::default();
    let part1 = bytes("foo\x1b[?10");
    let part2 = bytes("49hbar");
    alt_detect_process(&mut st, &part1);
    assert!(!st.active, "Should not enter until full sequence present");
    alt_detect_process(&mut st, &part2);
    assert!(st.active, "Should enter alt-screen after full sequence");

    let part3 = bytes("xxx\x1b[");
    let part4 = bytes("?1049l");
    alt_detect_process(&mut st, &part3);
    assert!(st.active, "Still active until leave completes");
    alt_detect_process(&mut st, &part4);
    assert!(!st.active, "Should leave alt-screen after full sequence");

    let mut st2 = AltDetectState::default();
    alt_detect_process(&mut st2, b"\x1b[?1047h");
    assert!(st2.active);
    alt_detect_process(&mut st2, b"\x1b[?1047l");
    assert!(!st2.active);

    let mut st3 = AltDetectState::default();
    alt_detect_process(&mut st3, b"\x1b[?47h");
    assert!(st3.active);
    alt_detect_process(&mut st3, b"\x1b[?47l");
    assert!(!st3.active);
  }
}
