use std::path::Path;

use anyhow::{Context, Result, bail};

use gix as git;
use gix::refs::transaction::PreviousValue;

use crate::config::AppContext;

use super::command::git as git_cmd;
use super::query::rev_parse;
use super::repo::open_main_repo;

/// Return the current HEAD branch name or "main" if unavailable.
///
/// Uses the main repository (not a linked worktree) and falls back to
/// "main" when HEAD cannot be resolved to a named branch.
pub fn head_branch(ctx: &AppContext) -> String {
  let Ok(repo) = open_main_repo(ctx.paths.root()) else {
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

pub fn current_branch_name(repo: &git::Repository) -> Result<String> {
  let head = repo.head().context("failed to resolve HEAD")?;
  match head.kind {
    git::head::Kind::Symbolic(r) => Ok(r.name.shorten().to_string()),
    git::head::Kind::Detached { .. } => bail!("detached HEAD: cannot determine base branch"),
    git::head::Kind::Unborn(name) => Ok(name.shorten().to_string()),
  }
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
  git_cmd(&["update-ref", &full, new_commit], cwd)
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
  git_cmd(&["branch", "-D", name], cwd)?;
  Ok(true)
}
