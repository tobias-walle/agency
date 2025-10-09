use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tracing::debug;

use super::registry::{registry, root_key};
use super::session::{spawn_reader_thread, PtySession};

pub fn spawn_command(
  project_root: &Path,
  task_id: u64,
  worktree_path: &Path,
  program: &str,
  args: &[String],
  env: &[(&str, &str)],
) -> anyhow::Result<()> {
  let mut reg = registry().lock().unwrap();
  let key = (root_key(project_root), task_id);
  if reg.sessions.contains_key(&key) {
    return Ok(());
  }

  debug!(
    event = "pty_spawn_prepare",
    task_id,
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
  debug!(event = "pty_spawn_openpty", task_id, rows = 24u16, cols = 80u16, "opened PTY pair");

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
    program,
    args_len = args.len(),
    env_len = env.len(),
    cwd = %worktree_path.display()
  );

  let session = Arc::new(PtySession::new(task_id, pair.master, child));
  spawn_reader_thread(Arc::clone(&session));
  reg.sessions.insert(key, session);
  Ok(())
}

pub fn ensure_spawn_sh(
  project_root: &Path,
  task_id: u64,
  worktree_path: &Path,
) -> anyhow::Result<()> {
  const EMPTY: [(&str, &str); 0] = [];
  spawn_command(project_root, task_id, worktree_path, "sh", &[], &EMPTY)
}
