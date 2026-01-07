use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::utils::daemon::stop_sessions_of_task;
use crate::utils::files::files_dir_for_task;
use crate::utils::git::{delete_branch_if_exists_at, prune_worktree_if_exists_at};

use super::metadata::TaskRef;
use super::paths::{branch_name, task_file, worktree_dir};

/// Clean up task artifacts: stop sessions, prune worktree, delete branch, remove task file and files directory.
///
/// # Errors
/// Returns an error if worktree pruning, branch deletion, or file removal fails.
pub fn cleanup_task_artifacts(
  ctx: &AppContext,
  task: &TaskRef,
  repo_workdir: &Path,
) -> Result<()> {
  // Best-effort stop of running sessions
  let _ = stop_sessions_of_task(ctx, task);

  let wt_dir = worktree_dir(&ctx.paths, task);
  let branch = branch_name(task);
  let file_path = task_file(&ctx.paths, task);
  let files_dir = files_dir_for_task(&ctx.paths, task);

  let _ = prune_worktree_if_exists_at(repo_workdir, &wt_dir);
  let _ = delete_branch_if_exists_at(repo_workdir, &branch)?;

  if file_path.exists() {
    fs::remove_file(&file_path)
      .with_context(|| format!("failed to remove {}", file_path.display()))?;
  }

  if files_dir.exists() {
    fs::remove_dir_all(&files_dir)
      .with_context(|| format!("failed to remove {}", files_dir.display()))?;
  }

  Ok(())
}
