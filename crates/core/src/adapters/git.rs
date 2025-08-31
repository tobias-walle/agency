use std::path::{Path, PathBuf};

use crate::adapters::fs;

/// Compute the branch name for a task: `orchestra/{id}-{slug}`
pub fn task_branch_name(id: u64, slug: &str) -> String {
  format!("orchestra/{}-{}", id, slug)
}

/// Compute the worktree path under `.orchestra/worktrees/{id}-{slug}`
pub fn task_worktree_path(project_root: &Path, id: u64, slug: &str) -> PathBuf {
  fs::worktrees_dir(project_root).join(format!("{}-{}", id, slug))
}

/// Ensure that the base branch exists in the repository and return its HEAD Oid.
/// Tries local branch first, then `origin/<branch>` remote tracking ref.
pub fn resolve_base_branch_tip(repo: &git2::Repository, base_branch: &str) -> Result<git2::Oid, git2::Error> {
  // Try local branch
  if let Ok(reference) = repo.find_reference(&format!("refs/heads/{}", base_branch)) {
    let oid = reference.target().ok_or_else(|| git2::Error::from_str("invalid reference target"))?;
    return Ok(oid);
  }
  // Try origin/<branch>
  if let Ok(reference) = repo.find_reference(&format!("refs/remotes/origin/{}", base_branch)) {
    let oid = reference.target().ok_or_else(|| git2::Error::from_str("invalid reference target"))?;
    return Ok(oid);
  }
  Err(git2::Error::from_str("base branch not found"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn naming_helpers() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    assert_eq!(task_branch_name(42, "feat-x"), "orchestra/42-feat-x");
    assert_eq!(task_worktree_path(root, 42, "feat-x"), root.join(".orchestra/worktrees/42-feat-x"));
  }
}
