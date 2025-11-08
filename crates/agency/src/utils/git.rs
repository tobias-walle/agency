use std::path::Path;

use anyhow::{Context, Result, bail};
use git2::{
  Branch, BranchType, Reference, Repository, Worktree, WorktreeAddOptions, WorktreePruneOptions,
};

pub fn open_main_repo(cwd: &Path) -> Result<Repository> {
  let repo = Repository::discover(cwd)?;
  if repo.is_worktree() {
    let main = Repository::open(repo.commondir())?;
    Ok(main)
  } else {
    Ok(repo)
  }
}

pub fn ensure_branch<'a>(repo: &'a Repository, name: &str) -> Result<Branch<'a>> {
  if repo.head_detached()? {
    bail!("detached HEAD: cannot create branch from detached HEAD");
  }
  let head = repo.head().context("failed to resolve HEAD")?;
  let commit = head
    .peel_to_commit()
    .context("failed to peel HEAD to commit")?;
  if let Ok(b) = repo.find_branch(name, BranchType::Local) {
    Ok(b)
  } else {
    let b = repo
      .branch(name, &commit, false)
      .context("failed to create branch")?;
    Ok(b)
  }
}

pub fn add_worktree(
  repo: &Repository,
  wt_name: &str,
  wt_path: &Path,
  branch_ref: &Reference,
) -> Result<Worktree> {
  if wt_path.exists() {
    bail!("worktree path {} already exists", wt_path.display());
  }
  if repo.find_worktree(wt_name).is_ok() {
    bail!("worktree {wt_name} already exists");
  }
  let mut opts = WorktreeAddOptions::new();
  opts.reference(Some(branch_ref));
  opts.checkout_existing(true);
  let wt = repo
    .worktree(wt_name, wt_path, Some(&opts))
    .context("failed to add worktree")?;
  Ok(wt)
}

pub fn remove_worktree_and_branch(
  repo: &Repository,
  wt_name: &str,
  branch_name: &str,
) -> Result<()> {
  if let Ok(wt) = repo.find_worktree(wt_name) {
    let mut opts = WorktreePruneOptions::new();
    opts.locked(true).valid(true).working_tree(true);
    wt.prune(Some(&mut opts))
      .context("failed to prune worktree")?;
  }
  if let Ok(mut branch) = repo.find_branch(branch_name, BranchType::Local) {
    branch.delete().context("failed to delete branch")?;
  }
  Ok(())
}

/// Returns the current branch's short name (e.g., "main").
/// Errors if HEAD is detached or cannot be resolved to a branch name.
pub fn current_branch_name(repo: &Repository) -> Result<String> {
  if repo.head_detached()? {
    bail!("detached HEAD: cannot determine base branch");
  }
  let head = repo.head().context("failed to resolve HEAD")?;
  let name = head
    .shorthand()
    .ok_or_else(|| anyhow::anyhow!("failed to obtain branch name"))?;
  Ok(name.to_string())
}
