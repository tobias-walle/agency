use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use gix as git;
use gix::refs::transaction::PreviousValue;

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

pub fn ensure_branch(repo: &git::Repository, name: &str) -> Result<String> {
  let full = format!("refs/heads/{name}");
  if repo.find_reference(&full).is_ok() {
    return Ok(name.to_string());
  }
  // Create branch at current HEAD commit
  let head = repo
    .head_commit()
    .context("failed to resolve HEAD commit")?;
  let commit_id = head.id();
  let _ = repo.reference(
    full.as_str(),
    commit_id,
    PreviousValue::MustNotExist,
    "create branch",
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
  match run_git(
    &[
      "worktree",
      "remove",
      "--force",
      wt_path.to_string_lossy().as_ref(),
    ],
    workdir,
  ) {
    Ok(()) => Ok(true),
    Err(_) => {
      let _ = run_git(&["worktree", "prune"], workdir);
      Ok(wt_path.exists())
    }
  }
}

pub fn add_worktree_for_branch(
  _repo: &git::Repository,
  _wt_name: &str,
  wt_path: &Path,
  _branch: &str,
) -> Result<()> {
  if wt_path.exists() {
    bail!("worktree path {} already exists", wt_path.display());
  }
  let workdir = _repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot add linked worktree"))?;
  run_git(
    &[
      "worktree",
      "add",
      "--quiet",
      wt_path.to_string_lossy().as_ref(),
      _branch,
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

pub fn rebase_onto(worktree_dir: &Path, base: &str) -> Result<()> {
  run_git(&["rebase", base], worktree_dir)
}

pub fn is_fast_forward(repo: &git::Repository, base: &str, task_branch: &str) -> Result<bool> {
  let workdir = repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot check fast-forward"))?;
  let status = std::process::Command::new("git")
    .current_dir(workdir)
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
  anyhow::bail!("git merge-base --is-ancestor failed: status={}", status);
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

pub fn update_branch_ref(repo: &git::Repository, branch: &str, new_commit: &str) -> Result<()> {
  let workdir = repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot update ref"))?;
  let full = format!("refs/heads/{branch}");
  run_git(&["update-ref", &full, new_commit], workdir)
}
