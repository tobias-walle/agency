use anyhow::Result;

use crate::config::AgencyPaths;
use crate::utils::files::local_files_path;
use crate::utils::task::{TaskRef, resolve_id_or_slug};

/// Detect the current task from the `AGENCY_TASK_ID` environment variable.
///
/// # Errors
/// Returns an error if not running in an agency context or task cannot be resolved.
pub fn detect_task_from_env(paths: &AgencyPaths) -> Result<TaskRef> {
  let task_id = std::env::var("AGENCY_TASK_ID")
    .map_err(|_| anyhow::anyhow!("Not running in agency context. Set AGENCY_TASK_ID or run inside a worktree"))?;
  resolve_id_or_slug(paths, &task_id)
}

/// Check if the current working directory is inside an agency worktree.
///
/// Detects this by checking if `.agency/local/files` exists (symlink or directory)
/// in the current directory or if we're inside a worktree directory.
pub fn is_in_worktree(paths: &AgencyPaths) -> bool {
  let local_files = local_files_path(paths.cwd());
  if local_files.exists() || local_files.is_symlink() {
    return true;
  }

  let worktrees_dir = paths.worktrees_dir();
  if !worktrees_dir.exists() {
    return false;
  }

  if let Ok(cwd_canonical) = paths.cwd().canonicalize()
    && let Ok(wt_canonical) = worktrees_dir.canonicalize()
  {
    return cwd_canonical.starts_with(&wt_canonical);
  }

  false
}
