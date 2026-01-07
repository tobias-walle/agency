use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::utils::child::run_child_process;

/// Run a `git` command while streaming stdout/stderr to the TUI sink when set, or
/// inheriting stdio in regular CLI mode. Fails if git exits with a non-zero status.
pub fn git(args: &[&str], cwd: &Path) -> Result<()> {
  let arg_vec: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
  let status = run_child_process("git", &arg_vec, cwd, &[])?;
  if !status.success() {
    bail!("git {} exited with status {}", args.join(" "), status);
  }
  Ok(())
}

pub(super) fn run_git(args: &[&str], cwd: &Path) -> Result<()> {
  // Run git quietly: suppress stdout/stderr to keep CLI logs clean.
  let status = std::process::Command::new("git")
    .current_dir(cwd)
    .args(args)
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .with_context(|| format!("failed to run git {}", args.join(" ")))?;
  if !status.success() {
    bail!("git {} exited with status {}", args.join(" "), status);
  }
  Ok(())
}

pub fn rebase_onto(worktree_dir: &Path, base: &str) -> Result<()> {
  // Stream rebase output to aid in debugging via TUI sink
  git(&["rebase", base], worktree_dir)
}

pub fn stash_push(workdir: &Path, message: &str) -> Result<Option<String>> {
  let output = std::process::Command::new("git")
    .current_dir(workdir)
    .args(["stash", "push", "--include-untracked", "--message", message])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .output()
    .with_context(|| "failed to run git stash push")?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("git stash push failed: {}", stderr.trim());
  }
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  if stdout.contains("No local changes to save") || stderr.contains("No local changes to save") {
    return Ok(None);
  }
  Ok(Some("stash@{0}".to_string()))
}

pub fn stash_pop(workdir: &Path, stash_ref: &str) -> Result<()> {
  let output = std::process::Command::new("git")
    .current_dir(workdir)
    .args(["stash", "pop", stash_ref])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .output()
    .with_context(|| format!("failed to run git stash pop {stash_ref}"))?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("git stash pop {stash_ref} failed: {}", stderr.trim());
  }
  Ok(())
}

/// Hard resets the checked-out main worktree to its HEAD within `cwd`.
pub fn hard_reset_to_head_at(cwd: &Path) -> Result<()> {
  git(&["reset", "--hard"], cwd)
}

#[cfg(test)]
mod tests {
  use super::{hard_reset_to_head_at, stash_pop, stash_push};
  use std::fs;

  fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
      .current_dir(cwd)
      .args(args)
      .status()
      .expect("spawn git");
    assert!(status.success(), "git {args:?} failed: {status:?}");
  }

  fn setup_test_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tmp");
    let root = dir.path();
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "Tester"]);
    run_git(root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "init"]);
    dir
  }

  #[test]
  fn stash_push_returns_none_when_no_changes() {
    let dir = setup_test_repo();
    let stash_ref = stash_push(dir.path(), "test stash").expect("stash");
    assert!(stash_ref.is_none(), "expected None when no changes");
  }

  #[test]
  fn stash_push_returns_ref_when_changes_present() {
    let dir = setup_test_repo();
    fs::write(dir.path().join("a.txt"), "modified\n").unwrap();
    let stash_ref = stash_push(dir.path(), "test stash").expect("stash");
    assert!(stash_ref.is_some(), "expected Some when changes present");
    assert_eq!(stash_ref.unwrap(), "stash@{0}");
  }

  #[test]
  fn stash_pop_restores_changes() {
    let dir = setup_test_repo();
    fs::write(dir.path().join("a.txt"), "modified\n").unwrap();
    let stash_ref = stash_push(dir.path(), "test").expect("stash").unwrap();
    let content_before = fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(content_before, "one\n", "stash should reset content");
    stash_pop(dir.path(), &stash_ref).expect("pop");
    let content_after = fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(content_after, "modified\n", "pop should restore content");
  }

  #[test]
  fn hard_reset_to_head_at_discards_changes() {
    let dir = setup_test_repo();
    fs::write(dir.path().join("a.txt"), "modified\n").unwrap();
    hard_reset_to_head_at(dir.path()).expect("reset");
    let content = fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(content, "one\n", "reset should discard changes");
  }
}
