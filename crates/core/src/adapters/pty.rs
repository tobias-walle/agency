// pty adapter: spawn a shell per task and allow single attachment
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use once_cell::sync::Lazy;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use uuid::Uuid;
use anyhow::Context;

const MAX_BUFFER_BYTES: usize = 1024 * 1024; // ~1 MiB cap for history ring
const ATTACH_REPLAY_BYTES: usize = 128 * 1024; // 128 KiB replay limit

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
  let root_key = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()).display().to_string();
  let key = (root_key, task_id);
  if reg.sessions.contains_key(&key) {
    return Ok(());
  }

  let pty_system = native_pty_system();
  let pair = pty_system
    .openpty(PtySize {
      rows: 24,
      cols: 80,
      pixel_width: 0,
      pixel_height: 0,
    })
    .with_context(|| format!("openpty failed for task {}", task_id))?;

  // Spawn a plain POSIX sh (no -l) into the pty with cwd set to worktree
  let mut cmd = CommandBuilder::new("sh");
  cmd.cwd(worktree_path.as_os_str());
  let child = pair
    .slave
    .spawn_command(cmd)
    .with_context(|| format!("spawn 'sh' in {}", worktree_path.display()))?;

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
            break;
          }
          Ok(n) => {
            let data = &tmp[..n];
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
          Err(_) => {
            sess_for_thread.eof.store(true, Ordering::SeqCst);
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

  let root_key = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()).display().to_string();
  let key = (root_key, task_id);
  reg.sessions.insert(key, session);
  Ok(())
}

/// Attach to a running PTY session for a task; returns an attachment id. Only one active attach.
pub fn attach(project_root: &Path, task_id: u64) -> anyhow::Result<String> {
  let mut reg = REGISTRY.lock().unwrap();
  let root_key = project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()).display().to_string();
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

    // Prefill outbox with tail of history ring for replay
    {
      let ring = sess.history_ring.lock().unwrap();
      let tail_start = if ring.len() > ATTACH_REPLAY_BYTES {
        ring.len() - ATTACH_REPLAY_BYTES
      } else {
        0
      };
      let replay_data = ring[tail_start..].to_vec();
      let mut outbox = sess.outbox.lock().unwrap();
      *outbox = Some(replay_data);
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
pub fn read(attachment_id: &str, max_bytes: Option<usize>, wait_ms: Option<u64>) -> anyhow::Result<(Vec<u8>, bool)> {
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
      if has_data || sess.eof.load(Ordering::SeqCst) { break; }
      let now = std::time::Instant::now();
      if now >= deadline { break; }
      let remaining = deadline - now;
      let (ref changed_lock, ref cv) = sess.cv;
      let guard = changed_lock.lock().unwrap();
      let _ = cv.wait_timeout(guard, remaining);
      // Loop and recheck outbox/EOF
    }
  }

  let mut outbox_opt = sess.outbox.lock().unwrap();
  let outbox = outbox_opt.as_mut().ok_or_else(|| anyhow::anyhow!("no outbox for attachment"))?;
  let take = max_bytes.unwrap_or(outbox.len());
  let n = take.min(outbox.len());
  let data = outbox.drain(..n).collect::<Vec<u8>>();
  let eof = sess.eof.load(Ordering::SeqCst);
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
  }
  let w = opt_writer.as_mut().unwrap();
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

/// Detach the active attachment.
pub fn detach(attachment_id: &str) -> anyhow::Result<()> {
  let mut reg = REGISTRY.lock().unwrap();
  if let Some(sess) = reg.attachments.remove(attachment_id) {
    let mut active = sess.active_attach.lock().unwrap();
    *active = None;
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
