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

pub struct PtySession {
  pub id: u64,
  master: Mutex<Box<dyn portable_pty::MasterPty + Send>>, // serialized access
  writer: Mutex<Option<Box<dyn Write + Send>>>,
  buffer: Mutex<Vec<u8>>, // accumulated output
  eof: AtomicBool,
  active_attach: Mutex<Option<String>>, // single attachment id
}

impl PtySession {
  fn new(id: u64, master: Box<dyn portable_pty::MasterPty + Send>) -> Self {
    Self {
      id,
      master: Mutex::new(master),
      writer: Mutex::new(None),
      buffer: Mutex::new(Vec::new()),
      eof: AtomicBool::new(false),
      active_attach: Mutex::new(None),
    }
  }
}

#[derive(Default)]
struct Registry {
  sessions: HashMap<u64, Arc<PtySession>>,       // task_id -> session
  attachments: HashMap<String, Arc<PtySession>>, // attach_id -> session
}

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| Mutex::new(Registry::default()));

/// Ensure a PTY session exists for the given task id; if not, spawn a default shell.
pub fn ensure_spawn(_project_root: &Path, task_id: u64) -> anyhow::Result<()> {
  let mut reg = REGISTRY.lock().unwrap();
  if reg.sessions.contains_key(&task_id) {
    return Ok(());
  }

  let pty_system = native_pty_system();
  let pair = pty_system.openpty(PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
  })?;

  // Spawn a shell into the pty (portable default: sh)
  let mut cmd = CommandBuilder::new("sh");
  cmd.arg("-l");
  let _child = pair.slave.spawn_command(cmd)?;

  let session = Arc::new(PtySession::new(task_id, pair.master));

  // Start reader thread that continuously reads from the master and appends to buffer
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
            let mut b = sess_for_thread.buffer.lock().unwrap();
            b.extend_from_slice(&tmp[..n]);
          }
          Err(_) => {
            sess_for_thread.eof.store(true, Ordering::SeqCst);
            break;
          }
        }
      }
    } else {
      sess_for_thread.eof.store(true, Ordering::SeqCst);
    }
  });

  reg.sessions.insert(task_id, session);
  Ok(())
}

/// Attach to a running PTY session for a task; returns an attachment id. Only one active attach.
pub fn attach(task_id: u64) -> anyhow::Result<String> {
  let mut reg = REGISTRY.lock().unwrap();
  let sess = reg
    .sessions
    .get(&task_id)
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
    Ok(id)
  }
}


/// Read and drain available output for an attachment.
pub fn read(attachment_id: &str, max_bytes: Option<usize>) -> anyhow::Result<(Vec<u8>, bool)> {
  let reg = REGISTRY.lock().unwrap();
  let sess = reg
    .attachments
    .get(attachment_id)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
  drop(reg);

  let mut buf = sess.buffer.lock().unwrap();
  let take = max_bytes.unwrap_or(buf.len());
  let n = take.min(buf.len());
  let data = buf.drain(..n).collect::<Vec<u8>>();
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
  w.flush()?;
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
  }
  Ok(())
}
