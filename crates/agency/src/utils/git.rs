use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use gix as git;
use gix::refs::transaction::PreviousValue;

use crate::utils::child::run_child_process;

/// Resolve the main repository workdir for any given `cwd`.
///
/// - When `cwd` is inside a linked worktree, returns the main repo's workdir.
/// - When `cwd` is inside a regular repo, returns that repo's workdir.
/// - When not inside any git repo, falls back to `cwd`.
#[must_use]
pub fn resolve_main_workdir(cwd: &Path) -> PathBuf {
  match git::discover(cwd) {
    Ok(repo) => match repo.kind() {
      git::repository::Kind::WorkTree { is_linked } if is_linked => {
        // Only redirect to the main repo when inside a linked worktree.
        repo
          .main_repo()
          .ok()
          .and_then(|r| r.workdir().map(std::path::Path::to_path_buf))
          .unwrap_or_else(|| cwd.to_path_buf())
      }
      _ => cwd.to_path_buf(),
    },
    Err(_) => cwd.to_path_buf(),
  }
}

pub fn open_main_repo(cwd: &Path) -> Result<git::Repository> {
  let repo = git::discover(cwd)?;
  match repo.kind() {
    git::repository::Kind::WorkTree { is_linked } if is_linked => {
      let main = repo.main_repo().context("failed to open main repo")?;
      Ok(main)
    }
    _ => Ok(repo),
  }
}

pub fn repo_workdir_or(repo: &git::Repository, fallback: &Path) -> PathBuf {
  repo.workdir().map_or_else(
    || fallback.to_path_buf(),
    |p| p.canonicalize().unwrap_or_else(|_| p.to_path_buf()),
  )
}

/// Return the current HEAD branch name or "main" if unavailable.
///
/// Uses the main repository (not a linked worktree) and falls back to
/// "main" when HEAD cannot be resolved to a named branch.
pub fn head_branch(ctx: &crate::config::AppContext) -> String {
  let Ok(repo) = open_main_repo(ctx.paths.cwd()) else {
    return "main".to_string();
  };
  match current_branch_name(&repo) {
    Ok(name) => name,
    Err(_) => "main".to_string(),
  }
}

/// Ensure a branch exists at the given starting point (rev or ref).
///
/// - If the branch already exists, returns its name without modifying it.
/// - Otherwise resolves `start_point` (rev-parse) and creates the branch at that commit.
pub fn ensure_branch_at(repo: &git::Repository, name: &str, start_point: &str) -> Result<String> {
  let full = format!("refs/heads/{name}");
  if repo.find_reference(&full).is_ok() {
    return Ok(name.to_string());
  }
  // Resolve start_point to a commit id via rev-parse in the main workdir
  let workdir = repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot create branch"))?;
  let commit = rev_parse(workdir, start_point)?;
  let oid = gix::ObjectId::from_hex(commit.as_bytes())
    .map_err(|_| anyhow::anyhow!("invalid commit id from rev-parse: {commit}"))?;
  let _ = repo.reference(
    full.as_str(),
    oid,
    PreviousValue::MustNotExist,
    "create branch at start point",
  )?;
  Ok(name.to_string())
}

pub fn delete_branch_if_exists(repo: &git::Repository, name: &str) -> Result<bool> {
  let full = format!("refs/heads/{name}");
  match repo.find_reference(&full) {
    Ok(r) => {
      r.delete()?;
      Ok(true)
    }
    Err(_) => Ok(false),
  }
}

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

pub fn current_branch_name(repo: &git::Repository) -> Result<String> {
  let head = repo.head().context("failed to resolve HEAD")?;
  match head.kind {
    git::head::Kind::Symbolic(r) => Ok(r.name.shorten().to_string()),
    git::head::Kind::Detached { .. } => bail!("detached HEAD: cannot determine base branch"),
    git::head::Kind::Unborn(name) => Ok(name.shorten().to_string()),
  }
}

fn run_git(args: &[&str], cwd: &Path) -> Result<()> {
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

/// Run a `git` command while streaming stdout/stderr to the TUI sink when set, or
/// inheriting stdio in regular CLI mode. Fails if git exits with a non-zero status.
pub fn git(args: &[&str], cwd: &Path) -> Result<()> {
  let argv: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
  let status = run_child_process("git", &argv, cwd, &[])?;
  if !status.success() {
    bail!("git {} exited with status {}", args.join(" "), status);
  }
  Ok(())
}

pub fn rebase_onto(worktree_dir: &Path, base: &str) -> Result<()> {
  // Stream rebase output to aid in debugging via TUI sink
  git(&["rebase", base], worktree_dir)
}

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
  anyhow::bail!("git merge-base --is-ancestor failed: status={status}");
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
    anyhow::bail!("git rev-parse failed: status={}", out.status);
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Resolve the top-level working directory for the repository that contains `cwd`.
pub fn git_workdir(cwd: &Path) -> Result<PathBuf> {
  let out = std::process::Command::new("git")
    .current_dir(cwd)
    .args(["rev-parse", "--show-toplevel"])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .with_context(|| "failed to spawn git rev-parse --show-toplevel")?
    .wait_with_output()
    .with_context(|| "failed to wait for git rev-parse --show-toplevel")?;
  if !out.status.success() {
    anyhow::bail!(
      "git rev-parse --show-toplevel failed: status={}",
      out.status
    );
  }
  Ok(PathBuf::from(
    String::from_utf8_lossy(&out.stdout).trim().to_string(),
  ))
}

/// Return the current branch name if HEAD points to a branch; otherwise Ok(None) (e.g. detached).
pub fn current_branch_name_at(cwd: &Path) -> Result<Option<String>> {
  let out = std::process::Command::new("git")
    .current_dir(cwd)
    .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .with_context(|| "failed to spawn git symbolic-ref --short HEAD")?
    .wait_with_output()
    .with_context(|| "failed to wait for git symbolic-ref --short HEAD")?;
  if out.status.success() {
    return Ok(Some(
      String::from_utf8_lossy(&out.stdout).trim().to_string(),
    ));
  }
  // Exit code 1 often indicates detached HEAD; treat as None
  if out.status.code() == Some(1) {
    return Ok(None);
  }
  anyhow::bail!(
    "git symbolic-ref --short HEAD failed: status={}",
    out.status
  );
}

/// Update a local branch ref to point at `new_commit` within `cwd`.
pub fn update_branch_ref_at(cwd: &Path, branch: &str, new_commit: &str) -> Result<()> {
  let full = format!("refs/heads/{branch}");
  git(&["update-ref", &full, new_commit], cwd)
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

/// Returns true if the main worktree has no changes (including untracked files).
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
    anyhow::bail!("git status --porcelain failed: status={}", out.status);
  }
  Ok(String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Hard resets the checked-out main worktree to its HEAD.
/// Hard resets the checked-out main worktree to its HEAD within `cwd`.
pub fn hard_reset_to_head_at(cwd: &Path) -> Result<()> {
  git(&["reset", "--hard"], cwd)
}

/// Delete a branch if it exists; returns Ok(true) if deleted, Ok(false) if it didn't exist.
pub fn delete_branch_if_exists_at(cwd: &Path, name: &str) -> Result<bool> {
  let full = format!("refs/heads/{name}");
  let status = std::process::Command::new("git")
    .current_dir(cwd)
    .args(["show-ref", "--quiet", "--verify", &full])
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .with_context(|| "failed to run git show-ref --verify")?;
  if status.code() == Some(1) {
    return Ok(false);
  }
  if !status.success() {
    anyhow::bail!("git show-ref --verify failed: status={status}");
  }
  git(&["branch", "-D", name], cwd)?;
  Ok(true)
}

// Tests moved to end of file to avoid items-after-test-module lint

/// Remove a linked worktree directory if it exists; returns whether it existed beforehand.
pub fn prune_worktree_if_exists_at(cwd: &Path, wt_path: &Path) -> Result<bool> {
  if !wt_path.exists() {
    return Ok(false);
  }
  if let Err(_e) = git(
    &[
      "worktree",
      "remove",
      "--force",
      wt_path.to_string_lossy().as_ref(),
    ],
    cwd,
  ) {
    let _ = git(&["worktree", "prune"], cwd);
    return Ok(wt_path.exists());
  }
  Ok(true)
}

#[cfg(test)]
mod tests {
  use super::resolve_main_workdir;
  use std::fs;
  use std::path::PathBuf;

  fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
      .current_dir(cwd)
      .args(args)
      .status()
      .expect("spawn git");
    assert!(status.success(), "git {args:?} failed: {status:?}");
  }

  #[test]
  fn resolves_fallback_when_not_a_repo() {
    let dir = tempfile::tempdir().expect("tmp");
    let got = resolve_main_workdir(dir.path());
    let got = got.canonicalize().unwrap_or(got);
    let want = dir
      .path()
      .canonicalize()
      .unwrap_or_else(|_| PathBuf::from(dir.path()));
    assert_eq!(got, want);
  }

  #[test]
  fn resolves_main_root_from_linked_worktree() {
    let root = tempfile::tempdir().expect("root");
    let root_path = root.path();

    // init repo and initial commit
    run_git(root_path, &["init"]);
    run_git(root_path, &["config", "user.email", "test@example.com"]);
    run_git(root_path, &["config", "user.name", "Tester"]);
    fs::write(root_path.join("README.md"), "ok\n").expect("write");
    run_git(root_path, &["add", "."]);
    run_git(root_path, &["commit", "-m", "init"]);

    // create linked worktree
    let wt_dir = root_path.join("wt");
    run_git(
      root_path,
      &["worktree", "add", wt_dir.to_str().unwrap(), "-b", "feature"],
    );

    let got = resolve_main_workdir(&wt_dir);
    let want = root_path
      .canonicalize()
      .unwrap_or_else(|_| PathBuf::from(root_path));
    assert_eq!(got, want);

    // cleanup best-effort
    let _ = fs::remove_dir_all(&wt_dir);
  }
}
