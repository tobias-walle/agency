use std::path::{Path, PathBuf};

use crate::adapters::fs;

/// Compute the branch name for a task: `agency/{id}-{slug}`
pub fn task_branch_name(id: u64, slug: &str) -> String {
  format!("agency/{}-{}", id, slug)
}

/// Compute the worktree path under `.agency/worktrees/{id}-{slug}`
pub fn task_worktree_path(project_root: &Path, id: u64, slug: &str) -> PathBuf {
  fs::worktrees_dir(project_root).join(format!("{}-{}", id, slug))
}

/// Ensure that the base branch exists in the repository and return its HEAD Oid.
/// Tries local branch first, then `origin/<branch>` remote tracking ref.
pub fn resolve_base_branch_tip(
  repo: &git2::Repository,
  base_branch: &str,
) -> Result<git2::Oid, git2::Error> {
  // Try local branch
  if let Ok(reference) = repo.find_reference(&format!("refs/heads/{}", base_branch)) {
    let oid = reference
      .target()
      .ok_or_else(|| git2::Error::from_str("invalid reference target"))?;
    return Ok(oid);
  }
  // Try origin/<branch>
  if let Ok(reference) = repo.find_reference(&format!("refs/remotes/origin/{}", base_branch)) {
    let oid = reference
      .target()
      .ok_or_else(|| git2::Error::from_str("invalid reference target"))?;
    return Ok(oid);
  }
  Err(git2::Error::from_str("base branch not found"))
}

/// Ensure a git worktree exists for the given task at `.agency/worktrees/{id}-{slug}`
/// and is checked out to branch `agency/{id}-{slug}` starting at the base branch tip.
/// Returns the path to the worktree on success.
pub fn ensure_task_worktree(
  repo: &git2::Repository,
  project_root: &Path,
  id: u64,
  slug: &str,
  base_branch: &str,
) -> anyhow::Result<PathBuf> {
  use anyhow::{Context, anyhow};
  use git2::Repository;

  let branch_name = task_branch_name(id, slug);
  let worktree_path = task_worktree_path(project_root, id, slug);
  let parent = worktree_path
    .parent()
    .ok_or_else(|| anyhow!("invalid worktree parent path"))?;
  std::fs::create_dir_all(parent)
    .with_context(|| format!("create parent worktrees dir {}", parent.display()))?;

  // Resolve base tip and ensure the task branch exists at that commit
  let base_oid = resolve_base_branch_tip(repo, base_branch)
    .with_context(|| format!("resolve base branch tip: {}", base_branch))?;
  let base_commit = repo
    .find_commit(base_oid)
    .with_context(|| format!("find commit for base tip {}", base_oid))?;

  // Create branch if missing
  let full_ref = format!("refs/heads/{}", branch_name);
  let branch_ref = match repo.find_reference(&full_ref) {
    Ok(r) => r,
    Err(_) => {
      let _b = repo
        .branch(&branch_name, &base_commit, false)
        .with_context(|| format!("create branch {} at {}", branch_name, base_oid))?;
      repo
        .find_reference(&full_ref)
        .with_context(|| format!("re-find created branch ref {}", full_ref))?
    }
  };

  // If worktree path already exists, validate it's a git worktree on expected branch
  if worktree_path.exists() {
    let wt_repo = Repository::open(&worktree_path).with_context(|| {
      format!(
        "existing dir at {} is not a git repository",
        worktree_path.display()
      )
    })?;
    let head = wt_repo.head().with_context(|| "read worktree HEAD")?;
    if !head.is_branch() {
      return Err(anyhow!(
        "existing worktree at {} is not on a branch",
        worktree_path.display()
      ));
    }
    let got = head.shorthand().unwrap_or("");
    if got != branch_name {
      return Err(anyhow!(
        "worktree branch mismatch at {}: expected {}, found {}",
        worktree_path.display(),
        branch_name,
        got
      ));
    }
    return Ok(worktree_path);
  }

  // Otherwise, create a new linked worktree
  // Use the worktree name as `{id}-{slug}` to match directory name
  let wt_name = format!("{}-{}", id, slug);
  // Best-effort: remove any stale empty dir before creating (libgit2 requires non-existent path)
  if worktree_path.exists() {
    // We shouldn't be here due to early return, but guard anyway
    return Err(anyhow!(
      "cannot create worktree: path already exists at {}",
      worktree_path.display()
    ));
  }
  let mut opts = git2::WorktreeAddOptions::new();
  // Try to associate the new worktree with the task branch reference if supported
  // Some libgit2 versions require setting the reference explicitly so HEAD points to it.
  opts.reference(Some(&branch_ref));
  let _wt = repo
    .worktree(&wt_name, &worktree_path, Some(&opts))
    .with_context(|| format!("create worktree {} at {}", wt_name, worktree_path.display()))?;

  // Open the newly created worktree repo and ensure HEAD is set to the task branch
  let wt_repo = Repository::open(&worktree_path)
    .with_context(|| format!("open created worktree repo at {}", worktree_path.display()))?;
  wt_repo
    .set_head(&full_ref)
    .with_context(|| format!("set worktree HEAD to {}", full_ref))?;

  // Force checkout to populate files according to HEAD
  let mut cb = git2::build::CheckoutBuilder::new();
  cb.force();
  wt_repo
    .checkout_head(Some(&mut cb))
    .with_context(|| "checkout HEAD in worktree")?;

  Ok(worktree_path)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn naming_helpers() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    assert_eq!(task_branch_name(42, "feat-x"), "agency/42-feat-x");
    assert_eq!(
      task_worktree_path(root, 42, "feat-x"),
      root.join(".agency/worktrees/42-feat-x")
    );
  }
}
