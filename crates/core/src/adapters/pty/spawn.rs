use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

use anyhow::{Context, anyhow};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tracing::{debug, info, warn};

use crate::adapters::fs as fsutil;
use crate::domain::task::{Status, Task, TaskId};

use super::registry::{registry, root_key};
use super::session::{PtySession, spawn_reader_thread};

pub fn spawn_command(
  project_root: &Path,
  task_id: u64,
  slug: &str,
  worktree_path: &Path,
  program: &str,
  args: &[String],
  env: &[(&str, &str)],
) -> anyhow::Result<()> {
  let mut reg = registry().lock().unwrap();
  let registry_root = root_key(project_root);
  let key = (registry_root.clone(), task_id);
  if reg.sessions.contains_key(&key) {
    return Ok(());
  }

  debug!(
    event = "pty_spawn_prepare",
    task_id,
    slug = %slug,
    program,
    args_len = args.len(),
    env_len = env.len(),
    worktree = %worktree_path.display()
  );

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

  let mut cmd = CommandBuilder::new(program);
  cmd.cwd(worktree_path.as_os_str());
  for arg in args {
    cmd.arg(arg);
  }
  for (name, value) in env {
    cmd.env(name, value);
  }
  let child = pair
    .slave
    .spawn_command(cmd)
    .with_context(|| format!("spawn '{}' in {}", program, worktree_path.display()))?;
  debug!(
    event = "pty_spawn_child",
    task_id,
    slug = %slug,
    program,
    args_len = args.len(),
    env_len = env.len(),
    cwd = %worktree_path.display()
  );

  let session = Arc::new(PtySession::new(
    task_id,
    slug.to_string(),
    project_root.to_path_buf(),
    registry_root,
    pair.master,
    child,
  ));
  spawn_reader_thread(Arc::clone(&session));
  spawn_reaper_thread(Arc::clone(&session));
  reg.sessions.insert(key, session);
  Ok(())
}

fn spawn_reaper_thread(session: Arc<PtySession>) {
  thread::spawn(move || {
    let maybe_child = {
      let mut child_slot = session.child.lock().unwrap();
      child_slot.take()
    };

    if let Some(mut child) = maybe_child {
      match child.wait() {
        Ok(status) => {
          let success = status.success();
          info!(
            event = "pty_child_exit",
            task_id = session.id,
            slug = %session.slug,
            success,
            status = ?status,
            "agent process exited"
          );
        }
        Err(error) => {
          warn!(
            event = "pty_child_wait_failed",
            task_id = session.id,
            slug = %session.slug,
            error = %error,
            "failed to wait on agent process"
          );
        }
      }
    } else {
      warn!(
        event = "pty_child_missing",
        task_id = session.id,
        slug = %session.slug,
        "child handle missing when waiting for agent exit"
      );
    }

    session.eof.store(true, Ordering::SeqCst);
    {
      let mut writer = session.writer.lock().unwrap();
      *writer = None;
    }
    {
      let mut active = session.active_attach.lock().unwrap();
      *active = None;
    }
    {
      let mut outbox = session.outbox.lock().unwrap();
      *outbox = None;
    }
    {
      let (ref changed_lock, ref cv) = session.cv;
      let mut changed = changed_lock.lock().unwrap();
      *changed = true;
      cv.notify_all();
    }

    {
      let mut reg = registry().lock().unwrap();
      reg
        .sessions
        .remove(&(session.registry_root.clone(), session.id));
      reg
        .attachments
        .retain(|_, existing| !Arc::ptr_eq(existing, &session));
    }

    if let Err(error) = persist_task_stopped(&session) {
      warn!(
        event = "pty_child_mark_stopped_error",
        task_id = session.id,
        slug = %session.slug,
        error = %error,
        "failed to mark task as stopped after agent exit"
      );
    }
  });
}

fn persist_task_stopped(session: &PtySession) -> anyhow::Result<()> {
  let task_id = TaskId(session.id);
  let task_path =
    fsutil::tasks_dir(&session.project_root).join(Task::format_filename(task_id, &session.slug));
  let contents = fs::read_to_string(&task_path)
    .with_context(|| format!("read task file {}", task_path.display()))?;
  let mut task = Task::from_markdown(task_id, session.slug.clone(), &contents)
    .map_err(|error| anyhow!("parse task markdown: {error}"))?;

  if task.front_matter.status == Status::Running {
    task
      .transition_to(Status::Stopped)
      .map_err(|error| anyhow!("transition task status: {error}"))?;
    let updated = task
      .to_markdown()
      .map_err(|error| anyhow!("serialize task markdown: {error}"))?;
    fs::write(&task_path, updated)
      .with_context(|| format!("write task file {}", task_path.display()))?;
    info!(
      event = "pty_child_mark_stopped",
      task_id = session.id,
      slug = %session.slug,
      "marked task as stopped after agent exit"
    );
  } else {
    debug!(
      event = "pty_child_mark_stopped_skipped",
      task_id = session.id,
      slug = %session.slug,
      status = ?task.front_matter.status,
      "task not running at agent exit"
    );
  }

  Ok(())
}

pub fn ensure_spawn_sh(
  project_root: &Path,
  task_id: u64,
  slug: &str,
  worktree_path: &Path,
) -> anyhow::Result<()> {
  const EMPTY: [(&str, &str); 0] = [];
  spawn_command(
    project_root,
    task_id,
    slug,
    worktree_path,
    "sh",
    &[],
    &EMPTY,
  )
}
