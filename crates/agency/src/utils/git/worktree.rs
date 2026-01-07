use std::path::Path;

use anyhow::{Result, bail};

use gix as git;

use super::command::{git as git_cmd, run_git};

pub fn prune_worktree_if_exists(repo: &git::Repository, wt_path: &Path) -> Result<bool> {
  if !wt_path.exists() {
    return Ok(false);
  }
  let workdir = repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot remove linked worktree"))?;
  if let Ok(()) = run_git(
    &[
      "worktree",
      "remove",
      "--force",
      wt_path.to_string_lossy().as_ref(),
    ],
    workdir,
  ) {
    Ok(true)
  } else {
    let _ = run_git(&["worktree", "prune"], workdir);
    Ok(wt_path.exists())
  }
}

pub fn add_worktree_for_branch(
  repo: &git::Repository,
  _wt_name: &str,
  wt_path: &Path,
  branch: &str,
) -> Result<()> {
  if wt_path.exists() {
    bail!("worktree path {} already exists", wt_path.display());
  }
  let workdir = repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot add linked worktree"))?;
  run_git(&["worktree", "prune"], workdir)?;
  run_git(
    &[
      "worktree",
      "add",
      "--quiet",
      wt_path.to_string_lossy().as_ref(),
      branch,
    ],
    workdir,
  )?;
  Ok(())
}

/// Remove a linked worktree directory if it exists; returns whether it existed beforehand.
pub fn prune_worktree_if_exists_at(cwd: &Path, wt_path: &Path) -> bool {
  if !wt_path.exists() {
    return false;
  }
  if let Err(_e) = git_cmd(
    &[
      "worktree",
      "remove",
      "--force",
      wt_path.to_string_lossy().as_ref(),
    ],
    cwd,
  ) {
    let _ = git_cmd(&["worktree", "prune"], cwd);
    return wt_path.exists();
  }
  true
}
