use std::fmt;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use portable_pty::PtySize;
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

mod constants;
pub mod input_encode;
mod registry;
mod sanitize;
mod session;
mod spawn;

pub use spawn::{ensure_spawn_sh, spawn_command};

use constants::{ATTACH_REPLAY_BYTES, ATTACH_REPLAY_EMIT_BYTES};
use registry::{registry, root_key};
use sanitize::sanitize_with_counters;

#[cfg(test)]
pub use registry::clear_registry_for_tests;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttachmentId(pub String);

impl fmt::Display for AttachmentId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.0.fmt(f)
  }
}

impl From<String> for AttachmentId {
  fn from(s: String) -> Self {
    AttachmentId(s)
  }
}

impl From<&str> for AttachmentId {
  fn from(s: &str) -> Self {
    AttachmentId(s.to_string())
  }
}

impl AsRef<str> for AttachmentId {
  fn as_ref(&self) -> &str {
    &self.0
  }
}

pub fn attach(project_root: &Path, task_id: u64, prefill: bool) -> anyhow::Result<AttachmentId> {
  let mut reg = registry().lock().unwrap();
  let key = (root_key(project_root), task_id);
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
    let id = AttachmentId(Uuid::new_v4().to_string());
    *active = Some(id.0.clone());
    reg.attachments.insert(id.clone(), Arc::clone(&sess));
    debug!(event = "pty_attach_new", task_id = sess.id, attachment_id = %id);

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

    {
      let (ref changed_lock, ref cv) = sess.cv;
      let mut changed = changed_lock.lock().unwrap();
      *changed = true;
      cv.notify_all();
    }

    Ok(id)
  }
}

pub fn read(
  attachment_id: &AttachmentId,
  max_bytes: Option<usize>,
  wait_ms: Option<u64>,
) -> anyhow::Result<(Vec<u8>, bool)> {
  let reg = registry().lock().unwrap();
  let sess = reg
    .attachments
    .get(attachment_id)
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("invalid attachment"))?;
  drop(reg);

  if let Some(wait) = wait_ms {
    let deadline = Instant::now() + Duration::from_millis(wait);
    loop {
      let has_data = {
        let outbox_opt = sess.outbox.lock().unwrap();
        if let Some(ref outbox) = *outbox_opt {
          !outbox.is_empty()
        } else {
          true
        }
      };
      if has_data || sess.eof.load(Ordering::SeqCst) {
        break;
      }
      let now = Instant::now();
      if now >= deadline {
        break;
      }
      let remaining = deadline - now;
      let (ref changed_lock, ref cv) = sess.cv;
      let guard = changed_lock.lock().unwrap();
      let _ = cv.wait_timeout(guard, remaining);
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
  debug!(
    event = "pty_read_drain",
    attachment_id = %attachment_id,
    pre_len,
    drained = n,
    post_len,
    eof,
    wait_ms = ?wait_ms,
    max_bytes = ?max_bytes
  );
  Ok((data, eof))
}

pub fn input(attachment_id: &AttachmentId, data: &[u8]) -> anyhow::Result<()> {
  let reg = registry().lock().unwrap();
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
  let writer = opt_writer.as_mut().unwrap();
  debug!(
    event = "pty_input_write",
    task_id = sess.id,
    bytes = data.len()
  );
  writer.write_all(data)?;
  Ok(())
}

pub fn resize(attachment_id: &AttachmentId, rows: u16, cols: u16) -> anyhow::Result<()> {
  let reg = registry().lock().unwrap();
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

pub fn jiggle_resize(attachment_id: &AttachmentId, rows: u16, cols: u16) -> anyhow::Result<()> {
  let reg = registry().lock().unwrap();
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

pub fn detach(attachment_id: &AttachmentId) -> anyhow::Result<()> {
  let mut reg = registry().lock().unwrap();
  if let Some(sess) = reg.attachments.remove(attachment_id) {
    let mut active = sess.active_attach.lock().unwrap();
    *active = None;
    debug!(event = "pty_detach_clear", task_id = sess.id, attachment_id = %attachment_id);
    let mut outbox = sess.outbox.lock().unwrap();
    *outbox = None;
    let (ref changed_lock, ref cv) = sess.cv;
    let mut changed = changed_lock.lock().unwrap();
    *changed = true;
    cv.notify_all();
  }
  Ok(())
}
