use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use gix as git;

use crate::utils::error_messages;

/// Resolve the main repository workdir for any given `cwd`.
///
/// - When `cwd` is inside a linked worktree, returns the main repo's workdir.
/// - When `cwd` is inside a regular repo, returns that repo's workdir.
/// - When not inside any git repo, falls back to `cwd`.
#[must_use]
pub fn resolve_main_workdir(cwd: &Path) -> PathBuf {
  match git::discover(cwd) {
    Ok(repo) => {
      // Get the workdir for the discovered repo
      let workdir = match repo.workdir() {
        Some(dir) => dir.to_path_buf(),
        None => return cwd.to_path_buf(), // Bare repo fallback
      };

      // If in a linked worktree, navigate to main repo
      match repo.kind() {
        git::repository::Kind::WorkTree { is_linked } if is_linked => repo
          .main_repo()
          .ok()
          .and_then(|r| r.workdir().map(std::path::Path::to_path_buf))
          .unwrap_or(workdir),
        _ => workdir, // Return repo workdir (not cwd!)
      }
    }
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
    anyhow::bail!(error_messages::git_command_failed(
      "rev-parse --show-toplevel",
      out.status
    ));
  }
  Ok(PathBuf::from(
    String::from_utf8_lossy(&out.stdout).trim().to_string(),
  ))
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
    run_git(root_path, &["config", "commit.gpgsign", "false"]);
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

  #[test]
  fn resolves_main_root_from_external_worktree() {
    let main_repo = tempfile::tempdir().expect("main");
    let wt_location = tempfile::tempdir().expect("wt");

    // Setup main repo with initial commit
    run_git(main_repo.path(), &["init"]);
    run_git(
      main_repo.path(),
      &["config", "user.email", "test@example.com"],
    );
    run_git(main_repo.path(), &["config", "user.name", "Tester"]);
    run_git(main_repo.path(), &["config", "commit.gpgsign", "false"]);
    fs::write(main_repo.path().join("README.md"), "ok\n").expect("write");
    run_git(main_repo.path(), &["add", "."]);
    run_git(main_repo.path(), &["commit", "-m", "init"]);

    // Create worktree in SEPARATE directory (external)
    let wt_path = wt_location.path().join("external-wt");
    run_git(
      main_repo.path(),
      &["worktree", "add", wt_path.to_str().unwrap(), "-b", "feat"],
    );

    let got = resolve_main_workdir(&wt_path);
    let got = got.canonicalize().unwrap_or(got);
    let want = main_repo
      .path()
      .canonicalize()
      .unwrap_or_else(|_| PathBuf::from(main_repo.path()));
    assert_eq!(got, want);
  }

  #[test]
  fn resolves_repo_root_from_subdirectory() {
    let dir = tempfile::tempdir().expect("tmp");
    let root = dir.path();

    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "Tester"]);
    run_git(root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("README.md"), "ok\n").expect("write");
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "init"]);

    // Create and test from subdirectory
    let subdir = root.join("src").join("nested");
    fs::create_dir_all(&subdir).expect("mkdir");

    let got = resolve_main_workdir(&subdir);
    let got = got.canonicalize().unwrap_or(got);
    let want = root.canonicalize().unwrap_or_else(|_| PathBuf::from(root));
    assert_eq!(got, want);
  }

  #[test]
  fn git_workdir_returns_toplevel() {
    use super::git_workdir;
    let dir = tempfile::tempdir().expect("tmp");
    let root = dir.path();
    run_git(root, &["init"]);
    run_git(root, &["config", "user.email", "test@example.com"]);
    run_git(root, &["config", "user.name", "Tester"]);
    run_git(root, &["config", "commit.gpgsign", "false"]);
    fs::write(root.join("README.md"), "test\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "init"]);

    let subdir = root.join("src");
    fs::create_dir(&subdir).unwrap();

    let workdir = git_workdir(&subdir).expect("workdir");
    let want = root.canonicalize().unwrap_or_else(|_| PathBuf::from(root));
    assert_eq!(workdir, want);
  }

  #[test]
  fn git_workdir_fails_for_non_repo() {
    use super::git_workdir;
    let dir = tempfile::tempdir().expect("tmp");
    let result = git_workdir(dir.path());
    assert!(result.is_err(), "expected error for non-repo");
  }
}
