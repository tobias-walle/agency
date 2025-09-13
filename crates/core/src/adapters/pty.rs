// pty adapter: spawn a shell per task and allow single attachment
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Context;
use once_cell::sync::Lazy;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tracing::debug;
use uuid::Uuid;

const MAX_BUFFER_BYTES: usize = 1024 * 1024; // ~1 MiB cap for history ring
const ATTACH_REPLAY_BYTES: usize = 128 * 1024; // 128 KiB replay limit
const ATTACH_REPLAY_EMIT_BYTES: usize = 8 * 1024; // Emit up to 8 KiB on initial prefill
const ALT_TAIL_MAX: usize = 8; // lookbehind window for alt-screen detection

fn is_printable_ascii(b: u8) -> bool {
  b >= 0x20 && b != 0x7F
}

fn sanitize_with_counters(input: &[u8]) -> (Vec<u8>, usize, usize) {
  if input.is_empty() {
    return (Vec::new(), 0, 0);
  }
  let mut dropped_head = 0usize;
  let mut dropped_tail = 0usize;

  // Step 1: if not at a safe boundary, try to align to line boundary after the first \n
  let mut start = 0usize;
  let first = input[0];
  let at_safe = first == b'\n' || first == 0x1B || is_printable_ascii(first);
  if !at_safe && let Some(pos) = input.iter().position(|&b| b == b'\n') {
    start = pos.saturating_add(1);
  }

  // Step 2: head sanitize: drop until safe boundary
  // Safe boundary: \n, ESC, or printable ASCII. Additionally, drop a head that looks like mid-CSI (starts with '[' followed by params and optional final).
  while start < input.len() {
    let b = input[start];
    if b == b'\n' || b == 0x1B || is_printable_ascii(b) {
      // Special-case: mid-CSI without ESC at very start like "[31m...": drop the bracketed sequence
      if b == b'[' {
        // Scan until a potential final (0x40..0x7E) or end
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
        // Drop the mid-CSI head if it seems like a bracketed sequence
        dropped_head += i - start;
        start = i;
        continue;
      }
      break;
    }
    start += 1;
    dropped_head += 1;
  }

  // Step 3: copy with CR normalization (convert isolated \r to \n)
  let mut out = Vec::with_capacity(input.len().saturating_sub(start));
  let mut i = start;
  while i < input.len() {
    let b = input[i];
    if b == b'\r' {
      if i + 1 < input.len() && input[i + 1] == b'\n' {
        out.push(b'\r');
      } else {
        out.push(b'\n');
      }
      i += 1;
      continue;
    }
    out.push(b);
    i += 1;
  }

  // Step 4: tail sanitize: drop dangling ESC or incomplete CSI starting at last ESC
  if let Some(last_esc_pos) = out.iter().rposition(|&b| b == 0x1B) {
    // If ESC is the last byte, drop it
    if last_esc_pos == out.len() - 1 {
      out.truncate(last_esc_pos);
      dropped_tail += 1;
    } else {
      // If ESC is followed by '[' but no final (0x40..0x7E) appears, drop from ESC onwards
      if out.get(last_esc_pos + 1) == Some(&b'[') {
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
  }

  (out, dropped_head, dropped_tail)
}

#[cfg_attr(not(test), allow(dead_code))]
fn sanitize_replay(input: &[u8]) -> Vec<u8> {
  sanitize_with_counters(input).0
}

#[derive(Debug, Default, Clone)]
struct AltDetectState {
  active: bool,
  tail: Vec<u8>,
}

fn alt_detect_process(state: &mut AltDetectState, data: &[u8]) {
  // Build scan buffer: previous tail + current data
  let mut scan = Vec::with_capacity(state.tail.len() + data.len());
  scan.extend_from_slice(&state.tail);
  scan.extend_from_slice(data);

  // Helper to attempt to parse CSI ? <num> <final>
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
          let _ = before; // for potential future use
          // advance past this sequence
          i = j + 1;
          continue;
        }
      }
      // Incomplete or non-matching; move forward by one to resync
    }
    i += 1;
  }

  // Update tail to last up to ALT_TAIL_MAX bytes of scan
  if scan.len() > ALT_TAIL_MAX {
    state.tail = scan[scan.len() - ALT_TAIL_MAX..].to_vec();
  } else {
    state.tail = scan;
  }
}

pub struct PtySession {
  pub id: u64,
  master: Mutex<Box<dyn portable_pty::MasterPty + Send>>, // serialized access
  writer: Mutex<Option<Box<dyn Write + Send>>>,
  history_ring: Mutex<Vec<u8>>, // persistent bounded history (never drained)
  outbox: Mutex<Option<Vec<u8>>>, // per-attachment output buffer (drained on read)
  eof: AtomicBool,
  active_attach: Mutex<Option<String>>, // single attachment id
  #[allow(dead_code)]
  child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>, // retain child handle
  // Long-polling primitive: condvar paired with outbox and eof state
  cv: (Mutex<bool>, std::sync::Condvar),
  // Alt-screen tracking
  alt_screen_active: AtomicBool,
  alt_detect_tail: Mutex<Vec<u8>>, // last up to 8 bytes across read boundaries
}

impl PtySession {
  fn new(
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

#[derive(Default)]
struct Registry {
  // (canonical project root, task_id) -> session
  sessions: HashMap<(String, u64), Arc<PtySession>>,
  attachments: HashMap<String, Arc<PtySession>>, // attach_id -> session
}

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));

#[cfg(test)]
/// Clear the global registry for test isolation
pub fn clear_registry_for_tests() {
  let mut reg = REGISTRY.lock().unwrap();
  reg.sessions.clear();
  reg.attachments.clear();
}

/// Ensure a PTY session exists for the given task id; if not, spawn a default shell.
pub fn ensure_spawn(project_root: &Path, task_id: u64, worktree_path: &Path) -> anyhow::Result<()> {
  let mut reg = REGISTRY.lock().unwrap();
  let root_key = project_root
    .canonicalize()
    .unwrap_or_else(|_| project_root.to_path_buf())
    .display()
    .to_string();
  let key = (root_key, task_id);
  if reg.sessions.contains_key(&key) {
    return Ok(());
  }
  debug!(event = "pty_ensure_spawn", task_id, worktree = %worktree_path.display(), "ensuring PTY spawn");

  let pty_system = native_pty_system();
  let pair = pty_system
    .openpty(PtySize {
      rows: 24,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    })
    .with_context(|| format!("openpty failed for task {}", task_id))?;
  debug!(
    event = "pty_spawn_openpty",
    task_id,
    rows = 24u16,
    cols = 80u16,
    "opened PTY pair"
  );

  // Spawn a plain POSIX sh (no -l) into the pty with cwd set to worktree
  let mut cmd = CommandBuilder::new("sh");
  cmd.cwd(worktree_path.as_os_str());
  let child = pair
    .slave
    .spawn_command(cmd)
    .with_context(|| format!("spawn 'sh' in {}", worktree_path.display()))?;
  debug!(event = "pty_spawn_child", task_id, cwd = %worktree_path.display(), shell = "sh", "spawned child into PTY");

  let session = Arc::new(PtySession::new(task_id, pair.master, child));

  // Start reader thread that continuously reads from the master and appends to history_ring and outbox
  let sess_for_thread = Arc::clone(&session);
  thread::spawn(move || {
    let reader_res = {
      let master = sess_for_thread.master.lock().unwrap();
      master.try_clone_reader()
    };
    let mut tmp = [0u8; 8192];
    if let Ok(mut reader) = reader_res {
      loop {
        match reader.read(&mut tmp) {
          Ok(0) => {
            sess_for_thread.eof.store(true, Ordering::SeqCst);
            debug!(
              event = "pty_reader_eof",
              task_id = sess_for_thread.id,
              "PTY reader reached EOF"
            );
            break;
          }
          Ok(n) => {
            debug!(
              event = "pty_reader_read",
              task_id = sess_for_thread.id,
              bytes = n
            );
            let data = &tmp[..n];

            // Alt-screen detection over output stream
            {
              let before = sess_for_thread.alt_screen_active.load(Ordering::SeqCst);
              let mut state = AltDetectState {
                active: before,
                tail: sess_for_thread.alt_detect_tail.lock().unwrap().clone(),
              };
              alt_detect_process(&mut state, data);
              if state.active != before {
                if state.active {
                  tracing::info!(event = "pty_alt_screen_on", task_id = sess_for_thread.id);
                } else {
                  tracing::info!(event = "pty_alt_screen_off", task_id = sess_for_thread.id);
                }
                sess_for_thread
                  .alt_screen_active
                  .store(state.active, Ordering::SeqCst);
              } else {
                // store unchanged state too to keep tail current
                sess_for_thread
                  .alt_screen_active
                  .store(state.active, Ordering::SeqCst);
              }
              let mut tail = sess_for_thread.alt_detect_tail.lock().unwrap();
              *tail = state.tail;
            }

            // Append to history ring (bounded)
            {
              let mut ring = sess_for_thread.history_ring.lock().unwrap();
              ring.extend_from_slice(data);
              if ring.len() > MAX_BUFFER_BYTES {
                let excess = ring.len() - MAX_BUFFER_BYTES;
                ring.drain(0..excess);
              }
            }
            // Append to outbox if attached
            {
              let mut outbox_opt = sess_for_thread.outbox.lock().unwrap();
              if let Some(ref mut outbox) = *outbox_opt {
                outbox.extend_from_slice(data);
              }
            }
            // Notify waiters
            {
              let (ref changed_lock, ref cv) = sess_for_thread.cv;
              let mut changed = changed_lock.lock().unwrap();
              *changed = true;
              cv.notify_all();
            }
          }
          Err(e) => {
            sess_for_thread.eof.store(true, Ordering::SeqCst);
            debug!(event = "pty_reader_error", task_id = sess_for_thread.id, error = %e);
            let (ref changed_lock, ref cv) = sess_for_thread.cv;
            let mut changed = changed_lock.lock().unwrap();
            *changed = true;
            cv.notify_all();
            break;
          }
        }
      }
    } else {
      sess_for_thread.eof.store(true, Ordering::SeqCst);
    }
  });

  let root_key = project_root
    .canonicalize()
    .unwrap_or_else(|_| project_root.to_path_buf())
    .display()
    .to_string();
  let key = (root_key, task_id);
  reg.sessions.insert(key, session);
  Ok(())
}

/// Attach to a running PTY session for a task; returns an attachment id. Only one active attach.
pub fn attach(project_root: &Path, task_id: u64, prefill: bool) -> anyhow::Result<String> {
  let mut reg = REGISTRY.lock().unwrap();
  let root_key = project_root
    .canonicalize()
    .unwrap_or_else(|_| project_root.to_path_buf())
    .display()
    .to_string();
  let key = (root_key, task_id);
  let sess = reg
    .sessions
    .get(&key)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("no pty session for task"))?;
  {
    let mut active = sess.active_attach.lock().unwrap();
    if active.is_some() {
      anyhow::bail!("session already attached");
    }
    let id = Uuid::new_v4().to_string();
    *active = Some(id.clone());
    reg.attachments.insert(id.clone(), sess.clone());
    debug!(event = "pty_attach_new", task_id = sess.id, attachment_id = %id);

    // Prefill outbox with tail of history ring for replay (gated by alt-screen state)
    let effective_prefill = prefill && !sess.alt_screen_active.load(Ordering::SeqCst);
    if effective_prefill {
      let ring = sess.history_ring.lock().unwrap();
      let tail_start = if ring.len() > ATTACH_REPLAY_BYTES {
        ring.len() - ATTACH_REPLAY_BYTES
      } else {
        0
      };
      let (sanitized, dropped_head, dropped_tail) = sanitize_with_counters(&ring[tail_start..]);
      let replay_len = sanitized.len();
      let emit_start = replay_len.saturating_sub(ATTACH_REPLAY_EMIT_BYTES);
      let replay_data = sanitized[emit_start..].to_vec();
      tracing::info!(
        event = "pty_attach_replay_prefill",
        task_id = sess.id,
        replay_bytes = replay_data.len(),
        dropped_head,
        dropped_tail
      );
      let mut outbox = sess.outbox.lock().unwrap();
      *outbox = Some(replay_data);
    } else {
      let mut outbox = sess.outbox.lock().unwrap();
      *outbox = Some(Vec::new());
    }
    // Notify waiter in case a read is already waiting
    {
      let (ref changed_lock, ref cv) = sess.cv;
      let mut changed = changed_lock.lock().unwrap();
      *changed = true;
      cv.notify_all();
    }

    Ok(id)
  }
}

/// Read and drain available output for an attachment.
pub fn read(
  attachment_id: &str,
  max_bytes: Option<usize>,
  wait_ms: Option<u64>,
) -> anyhow::Result<(Vec<u8>, bool)> {
  let reg = REGISTRY.lock().unwrap();
  let sess = reg
    .attachments
    .get(attachment_id)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
  drop(reg);

  // If outbox empty and wait requested, block until data or timeout/EOF
  if let Some(wait) = wait_ms {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait);
    loop {
      // Fast path: data available or EOF -> stop waiting
      let has_data = {
        let outbox_opt = sess.outbox.lock().unwrap();
        if let Some(ref outbox) = *outbox_opt {
          !outbox.is_empty()
        } else {
          // No outbox (detached) -> stop waiting
          true
        }
      };
      if has_data || sess.eof.load(Ordering::SeqCst) {
        break;
      }
      let now = std::time::Instant::now();
      if now >= deadline {
        break;
      }
      let remaining = deadline - now;
      let (ref changed_lock, ref cv) = sess.cv;
      let guard = changed_lock.lock().unwrap();
      let _ = cv.wait_timeout(guard, remaining);
      // Loop and recheck outbox/EOF
    }
  }

  let mut outbox_opt = sess.outbox.lock().unwrap();
  let outbox = outbox_opt
    .as_mut()
    .ok_or_else(|| anyhow::anyhow!("no outbox for attachment"))?;
  let pre_len = outbox.len();
  let take = max_bytes.unwrap_or(outbox.len());
  let n = take.min(outbox.len());
  let data = outbox.drain(..n).collect::<Vec<u8>>();
  let post_len = outbox.len();
  let eof = sess.eof.load(Ordering::SeqCst);
  debug!(event = "pty_read_drain", attachment_id, pre_len, drained = n, post_len, eof, wait_ms = ?wait_ms, max_bytes = ?max_bytes);
  Ok((data, eof))
}

/// Send input to the PTY.
pub fn input(attachment_id: &str, data: &[u8]) -> anyhow::Result<()> {
  let reg = REGISTRY.lock().unwrap();
  let sess = reg
    .attachments
    .get(attachment_id)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
  drop(reg);

  let mut opt_writer = sess.writer.lock().unwrap();
  if opt_writer.is_none() {
    let master = sess.master.lock().unwrap();
    *opt_writer = Some(master.take_writer()?);
    debug!(
      event = "pty_writer_init",
      task_id = sess.id,
      "initialized writer for session"
    );
  }
  let w = opt_writer.as_mut().unwrap();
  debug!(
    event = "pty_input_write",
    task_id = sess.id,
    bytes = data.len()
  );
  w.write_all(data)?;
  // No explicit flush needed for PTY
  Ok(())
}

/// Resize the PTY
pub fn resize(attachment_id: &str, rows: u16, cols: u16) -> anyhow::Result<()> {
  let reg = REGISTRY.lock().unwrap();
  let sess = reg
    .attachments
    .get(attachment_id)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
  drop(reg);
  let master = sess.master.lock().unwrap();
  master.resize(PtySize {
    rows,
    cols,
    pixel_width: 0,
    pixel_height: 0,
  })?;
  Ok(())
}

/// Perform a minimal two-step resize to trigger TUI redraws without altering state.
pub fn jiggle_resize(attachment_id: &str, rows: u16, cols: u16) -> anyhow::Result<()> {
  let reg = REGISTRY.lock().unwrap();
  let sess = reg
    .attachments
    .get(attachment_id)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
  drop(reg);
  let master = sess.master.lock().unwrap();
  if cols > 1 {
    master.resize(PtySize {
      rows,
      cols: cols - 1,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    master.resize(PtySize {
      rows,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    })?;
  } else if rows > 1 {
    master.resize(PtySize {
      rows: rows - 1,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    })?;
    master.resize(PtySize {
      rows,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    })?;
  } else {
    master.resize(PtySize {
      rows,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    })?;
  }
  Ok(())
}

/// Detach the active attachment.
pub fn detach(attachment_id: &str) -> anyhow::Result<()> {
  let mut reg = REGISTRY.lock().unwrap();
  if let Some(sess) = reg.attachments.remove(attachment_id) {
    let mut active = sess.active_attach.lock().unwrap();
    *active = None;
    debug!(event = "pty_detach_clear", task_id = sess.id, attachment_id = %attachment_id);
    // Clear the outbox on detach
    let mut outbox = sess.outbox.lock().unwrap();
    *outbox = None;
    // Notify potential waiters to unblock
    let (ref changed_lock, ref cv) = sess.cv;
    let mut changed = changed_lock.lock().unwrap();
    *changed = true;
    cv.notify_all();
  }
  Ok(())
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

  #[test]
  fn alt_screen_detection_enters_and_leaves() {
    let mut st = AltDetectState::default();
    // Enter sequence split across boundary: "...\x1b[?1049h"
    let part1 = bytes("foo\x1b[?10");
    let part2 = bytes("49hbar");
    alt_detect_process(&mut st, &part1);
    assert!(!st.active, "Should not enter until full sequence present");
    alt_detect_process(&mut st, &part2);
    assert!(st.active, "Should enter alt-screen after full sequence");

    // Leave sequence split differently: "...\x1b[?1049l"
    let part3 = bytes("xxx\x1b[");
    let part4 = bytes("?1049l");
    alt_detect_process(&mut st, &part3);
    assert!(st.active, "Still active until leave completes");
    alt_detect_process(&mut st, &part4);
    assert!(!st.active, "Should leave alt-screen after full sequence");

    // Also support 1047 and 47
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
