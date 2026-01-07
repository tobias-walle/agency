use std::path::Path;

use anyhow::{Context, Result};

use crate::utils::error_messages;

/// Like `is_fast_forward` but operates directly on a working directory path.
pub fn is_fast_forward_at(cwd: &Path, base: &str, task_branch: &str) -> Result<bool> {
  let status = std::process::Command::new("git")
    .current_dir(cwd)
    .args(["merge-base", "--is-ancestor", base, task_branch])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .with_context(|| "failed to run git merge-base --is-ancestor")?;
  if status.success() {
    return Ok(true);
  }
  if status.code() == Some(1) {
    return Ok(false);
  }
  anyhow::bail!(error_messages::git_command_failed(
    "merge-base --is-ancestor",
    status
  ));
}

pub fn rev_parse(cwd: &Path, rev: &str) -> Result<String> {
  let out = std::process::Command::new("git")
    .current_dir(cwd)
    .arg("rev-parse")
    .arg(rev)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .with_context(|| "failed to spawn git rev-parse")?
    .wait_with_output()
    .with_context(|| "failed to wait for git rev-parse")?;
  if !out.status.success() {
    anyhow::bail!(error_messages::git_command_failed("rev-parse", out.status));
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Returns true if the working tree at `cwd` has no changes (including untracked files).
pub fn worktree_is_clean_at(cwd: &Path) -> Result<bool> {
  let out = std::process::Command::new("git")
    .current_dir(cwd)
    .arg("status")
    .arg("--porcelain")
    .arg("--untracked-files=no")
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .with_context(|| "failed to spawn git status --porcelain")?
    .wait_with_output()
    .with_context(|| "failed to wait for git status --porcelain")?;
  if !out.status.success() {
    anyhow::bail!(error_messages::git_command_failed(
      "status --porcelain",
      out.status
    ));
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Compute unstaged diffstat additions/deletions for the working tree at `workdir`.
/// Returns (additions, deletions). When there are no changes, returns (0,0).
pub fn uncommitted_numstat_at(workdir: &Path) -> Result<(u64, u64)> {
  let out = std::process::Command::new("git")
    .current_dir(workdir)
    .args(["diff", "--numstat"])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .with_context(|| "failed to spawn git diff --numstat")?
    .wait_with_output()
    .with_context(|| "failed to wait for git diff --numstat")?;
  if !out.status.success() {
    anyhow::bail!(error_messages::git_command_failed(
      "diff --numstat",
      out.status
    ));
  }
  let mut add: u64 = 0;
  let mut del: u64 = 0;
  for line in String::from_utf8_lossy(&out.stdout).lines() {
    // Format: "A\tD\tpath"; A or D can be '-' for binary; treat as 0
    let mut parts = line.split('\t');
    let a = parts.next().unwrap_or("0");
    let d = parts.next().unwrap_or("0");
    let a = a.parse::<u64>().unwrap_or(0);
    let d = d.parse::<u64>().unwrap_or(0);
    add = add.saturating_add(a);
    del = del.saturating_add(d);
  }
  Ok((add, del))
}

/// Count commits where `branch` is ahead of `base` within `repo_root`.
pub fn commits_ahead_at(repo_root: &Path, base: &str, branch: &str) -> Result<u64> {
  let range = format!("{base}..{branch}");
  let out = std::process::Command::new("git")
    .current_dir(repo_root)
    .args(["rev-list", "--count", &range])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .with_context(|| "failed to spawn git rev-list --count")?
    .wait_with_output()
    .with_context(|| "failed to wait for git rev-list --count")?;
  if !out.status.success() {
    anyhow::bail!(error_messages::git_command_failed(
      "rev-list --count",
      out.status
    ));
  }
  let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
  let n = s.parse::<u64>().unwrap_or(0);
  Ok(n)
}

#[cfg(test)]
mod tests {
  use super::{
    commits_ahead_at, is_fast_forward_at, rev_parse, uncommitted_numstat_at, worktree_is_clean_at,
  };
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
  fn uncommitted_numstat_reports_changes() {
    let dir = setup_test_repo();
    let root = dir.path();
    fs::write(root.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let (a, d) = uncommitted_numstat_at(root).expect("numstat");
    assert!(a >= 1, "expected at least 1 addition, got {a}");
    assert!(d <= 1, "expected at most 1 deletion, got {d}");
  }

  #[test]
  fn uncommitted_numstat_reports_zero_for_clean_tree() {
    let dir = setup_test_repo();
    let (a, d) = uncommitted_numstat_at(dir.path()).expect("numstat");
    assert_eq!(a, 0, "expected 0 additions");
    assert_eq!(d, 0, "expected 0 deletions");
  }

  #[test]
  fn commits_ahead_counts_range() {
    let dir = setup_test_repo();
    let root = dir.path();
    run_git(root, &["checkout", "-b", "feature"]);
    fs::write(root.join("b.txt"), "two\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "feat"]);
    let n = commits_ahead_at(root, "master", "feature")
      .or_else(|_| commits_ahead_at(root, "main", "feature"))
      .expect("count ahead");
    assert!(n >= 1, "expected at least 1 ahead, got {n}");
  }

  #[test]
  fn is_fast_forward_at_true_when_ancestor() {
    let dir = setup_test_repo();
    let root = dir.path();
    run_git(root, &["checkout", "-b", "feature"]);
    fs::write(root.join("b.txt"), "two\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "feat"]);
    let base = if std::process::Command::new("git")
      .current_dir(root)
      .args(["rev-parse", "--verify", "main"])
      .output()
      .map(|o| o.status.success())
      .unwrap_or(false)
    {
      "main"
    } else {
      "master"
    };
    let is_ff = is_fast_forward_at(root, base, "feature").expect("check ff");
    assert!(is_ff, "expected fast-forward from {base} to feature");
  }

  #[test]
  fn is_fast_forward_at_false_when_not_ancestor() {
    let dir = setup_test_repo();
    let root = dir.path();
    run_git(root, &["checkout", "-b", "branch1"]);
    fs::write(root.join("b.txt"), "b\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "b1"]);
    let base = if std::process::Command::new("git")
      .current_dir(root)
      .args(["rev-parse", "--verify", "main"])
      .output()
      .map(|o| o.status.success())
      .unwrap_or(false)
    {
      "main"
    } else {
      "master"
    };
    run_git(root, &["checkout", base]);
    run_git(root, &["checkout", "-b", "branch2"]);
    fs::write(root.join("c.txt"), "c\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "b2"]);
    let is_ff = is_fast_forward_at(root, "branch2", "branch1").expect("check ff");
    assert!(!is_ff, "expected no fast-forward between diverged branches");
  }

  #[test]
  fn rev_parse_resolves_head() {
    let dir = setup_test_repo();
    let commit = rev_parse(dir.path(), "HEAD").expect("rev-parse HEAD");
    assert_eq!(commit.len(), 40, "expected 40-char SHA");
  }

  #[test]
  fn rev_parse_fails_for_invalid_ref() {
    let dir = setup_test_repo();
    let result = rev_parse(dir.path(), "nonexistent-ref");
    assert!(result.is_err(), "expected error for invalid ref");
  }

  #[test]
  fn worktree_is_clean_at_true_for_clean_tree() {
    let dir = setup_test_repo();
    let clean = worktree_is_clean_at(dir.path()).expect("check clean");
    assert!(clean, "expected clean worktree");
  }

  #[test]
  fn worktree_is_clean_at_false_for_modified_files() {
    let dir = setup_test_repo();
    fs::write(dir.path().join("a.txt"), "modified\n").unwrap();
    let clean = worktree_is_clean_at(dir.path()).expect("check clean");
    assert!(!clean, "expected dirty worktree with modifications");
  }
}
