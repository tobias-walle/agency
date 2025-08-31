use std::fs;
use std::path::{Path, PathBuf};

/// Return path to the `.orchestra` folder inside the given project root
pub fn orchestra_dir(project_root: &Path) -> PathBuf {
  project_root.join(".orchestra")
}

/// Standard subpaths under `.orchestra`
pub fn logs_path(project_root: &Path) -> PathBuf {
  orchestra_dir(project_root).join("logs.jsonl")
}

pub fn tasks_dir(project_root: &Path) -> PathBuf {
  orchestra_dir(project_root).join("tasks")
}

pub fn worktrees_dir(project_root: &Path) -> PathBuf {
  orchestra_dir(project_root).join("worktrees")
}

/// Resolve the worktree path for a given task id and slug, as `worktrees/{id}-{slug}`
pub fn worktree_path(project_root: &Path, id: u64, slug: &str) -> PathBuf {
  worktrees_dir(project_root).join(format!("{}-{}", id, slug))
}

/// Ensure the `.orchestra` layout exists (directories are created if missing)
pub fn ensure_layout(project_root: &Path) -> std::io::Result<()> {
  fs::create_dir_all(tasks_dir(project_root))?;
  fs::create_dir_all(worktrees_dir(project_root))?;
  // logs file lazily created by logging subsystem; ensure parent exists
  fs::create_dir_all(orchestra_dir(project_root))?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn layout_paths() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    assert_eq!(orchestra_dir(root), root.join(".orchestra"));
    assert_eq!(logs_path(root), root.join(".orchestra/logs.jsonl"));
    assert_eq!(tasks_dir(root), root.join(".orchestra/tasks"));
    assert_eq!(worktrees_dir(root), root.join(".orchestra/worktrees"));
  }

  #[test]
  fn ensure_layout_creates_dirs() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    ensure_layout(root).unwrap();
    assert!(orchestra_dir(root).exists());
    assert!(tasks_dir(root).exists());
    assert!(worktrees_dir(root).exists());
  }

  #[test]
  fn worktree_path_is_under_worktrees() {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    let p = worktree_path(root, 42, "abc");
    assert_eq!(p, root.join(".orchestra/worktrees/42-abc"));
  }
}
