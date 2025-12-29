use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reflink_copy::reflink_or_copy;

use crate::config::{AppContext, BootstrapConfig};
use crate::log_info;
use crate::log_warn;
use crate::utils::child::run_child_process;
use crate::utils::cmd::{CmdCtx, expand_argv};
use crate::utils::files::{files_dir_for_task, local_files_path};
use crate::utils::task::TaskRef;
use gix as git;

/// Maximum file size for any bootstrap file copying (10MB)
const MAX_BOOTSTRAP_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Maximum file size for synchronous copying during worktree creation (100KB)
/// Files under this threshold are copied immediately; larger files are handled in background.
const SYNC_BOOTSTRAP_FILE_BYTES: u64 = 100 * 1024;

/// Copy small gitignored root files synchronously during worktree creation.
///
/// This ensures commonly-needed files like .env are available immediately
/// when the user attaches to the session, without waiting for background bootstrap.
pub fn sync_copy_small_gitignored_files(
  root_workdir: &Path,
  dst_worktree: &Path,
  cfg: &BootstrapConfig,
) -> Result<()> {
  copy_gitignored_root_files(root_workdir, dst_worktree, cfg, Some(SYNC_BOOTSTRAP_FILE_BYTES))
}

/// Bootstrap a worktree by copying larger gitignored files and included directories.
///
/// This runs in the background and handles files larger than the sync threshold,
/// as well as directories explicitly included in the bootstrap config.
pub fn bootstrap_worktree(
  root_workdir: &Path,
  dst_worktree: &Path,
  cfg: &BootstrapConfig,
) -> Result<()> {
  // Copy remaining large files (small ones were already copied synchronously)
  copy_gitignored_root_files(root_workdir, dst_worktree, cfg, None)?;

  // Copy explicitly included directories
  let entries = discover_root_entries(root_workdir)?;
  for entry in entries {
    let Ok(file_type) = entry.file_type() else {
      continue;
    };
    if !file_type.is_dir() {
      continue;
    }
    let name = entry.file_name().to_string_lossy().to_string();
    if is_excluded(&name, cfg) {
      continue;
    }
    // Only copy directories when explicitly included
    if cfg.include.iter().any(|inc| inc == &name) {
      let dst_dir = dst_worktree.join(&name);
      copy_dir_tree(&entry.path(), &dst_dir)?;
    }
  }

  Ok(())
}

/// Copy gitignored root files to the worktree.
///
/// If `max_size` is Some, only files under that size are copied.
/// Files that already exist in the destination are skipped.
fn copy_gitignored_root_files(
  root_workdir: &Path,
  dst_worktree: &Path,
  cfg: &BootstrapConfig,
  max_size: Option<u64>,
) -> Result<()> {
  let entries = discover_root_entries(root_workdir)?;

  // Split entries by type and filter out excluded names up front
  let mut file_names: Vec<String> = Vec::new();
  let mut file_paths: Vec<std::path::PathBuf> = Vec::new();
  for entry in entries {
    let Ok(file_type) = entry.file_type() else {
      continue;
    };
    if !file_type.is_file() {
      continue;
    }
    let name = entry.file_name().to_string_lossy().to_string();
    if is_excluded(&name, cfg) {
      continue;
    }
    file_names.push(name);
    file_paths.push(entry.path());
  }

  // Batch evaluate ignore status for root files once
  // Always use root_workdir for git check-ignore since that's where .gitignore lives
  let ignored = run_git_check_ignore_batch(
    root_workdir,
    &file_names
      .iter()
      .map(std::string::String::as_str)
      .collect::<Vec<_>>(),
  )?;
  let ignored_set: std::collections::HashSet<String> = ignored.into_iter().collect();

  for (idx, name) in file_names.iter().enumerate() {
    if !ignored_set.contains(name) {
      continue;
    }
    let src_path = &file_paths[idx];

    // Check file size against the limit
    let size = fs::metadata(src_path)
      .with_context(|| format!("stat {}", src_path.display()))?
      .len();

    // Skip files over the max bootstrap limit
    if size > MAX_BOOTSTRAP_FILE_BYTES {
      continue;
    }

    // If max_size is specified, only copy files within that size
    if let Some(max) = max_size
      && size > max
    {
      continue;
    }

    let dst_path = dst_worktree.join(name);
    if dst_path.exists() {
      continue;
    }
    copy_file(src_path, &dst_path)?;
  }

  Ok(())
}

/// Result from creating a worktree.
pub struct CreateWorktreeResult {
  pub worktree_dir: PathBuf,
  /// True if the worktree was newly created (bootstrap will run in background)
  pub is_new: bool,
}

/// Create a worktree for a task (fast, synchronous).
///
/// This creates the git worktree, files symlink, and copies small gitignored files
/// (like .env) synchronously. Larger files and directories are handled by
/// `run_bootstrap_in_worktree` in the background.
///
/// Returns the worktree path and whether background bootstrap is needed.
pub fn create_worktree_for_task(
  ctx: &AppContext,
  repo: &git::Repository,
  task: &TaskRef,
  branch: &str,
) -> anyhow::Result<CreateWorktreeResult> {
  use crate::utils::git::{add_worktree_for_branch, repo_workdir_or};
  use crate::utils::task::{worktree_dir, worktree_name};
  use anyhow::Context as _;

  let worktree_dir_path = worktree_dir(&ctx.paths, task);
  let is_new = if worktree_dir_path.exists() {
    false
  } else {
    let wt_root = ctx.paths.worktrees_dir();
    std::fs::create_dir_all(&wt_root)
      .with_context(|| format!("failed to create {}", wt_root.display()))?;
    add_worktree_for_branch(repo, &worktree_name(task), &worktree_dir_path, branch)?;
    true
  };

  create_files_symlink(&ctx.paths, task, &worktree_dir_path);

  // Copy small gitignored files synchronously (like .env) so they're available immediately
  if is_new {
    let repo_root = repo_workdir_or(repo, ctx.paths.root());
    let bcfg = ctx.config.bootstrap_config();
    if let Err(err) = sync_copy_small_gitignored_files(&repo_root, &worktree_dir_path, &bcfg) {
      log_warn!("Failed to copy small gitignored files: {err}");
    }
  }

  let canonical = worktree_dir_path
    .canonicalize()
    .unwrap_or(worktree_dir_path.clone());

  Ok(CreateWorktreeResult {
    worktree_dir: canonical,
    is_new,
  })
}

/// Run bootstrap operations in a worktree (slow, meant for background execution).
///
/// This copies gitignored files and runs the bootstrap command.
/// Receives pre-built environment variables from the caller.
pub fn run_bootstrap_in_worktree(
  repo_root: &Path,
  worktree_dir: &Path,
  cfg: &BootstrapConfig,
  env_vars: &std::collections::HashMap<String, String>,
) -> anyhow::Result<()> {
  bootstrap_worktree(repo_root, worktree_dir, cfg)?;
  run_bootstrap_cmd_with_env(repo_root, worktree_dir, cfg, env_vars);
  Ok(())
}

/// Run the configured bootstrap command with custom environment variables.
fn run_bootstrap_cmd_with_env(
  repo_root: &Path,
  worktree_dir: &Path,
  cfg: &BootstrapConfig,
  env_vars: &std::collections::HashMap<String, String>,
) {
  if cfg.cmd.is_empty() {
    return;
  }

  let root_abs = repo_root
    .canonicalize()
    .unwrap_or_else(|_| repo_root.to_path_buf())
    .display()
    .to_string();
  let ctx = CmdCtx::with_env(root_abs.clone(), env_vars.clone());
  let argv = expand_argv(&cfg.cmd, &ctx);

  // Special-case: default path missing should be a silent skip
  if cfg.cmd.len() == 1 && cfg.cmd[0] == "<root>/.agency/setup.sh" {
    let candidate = PathBuf::from(&argv[0]);
    if !candidate.exists() {
      return;
    }
  }

  log_info!("Run bootstrap {}", argv.join(" "));
  let env_overrides: Vec<(String, String)> = env_vars
    .iter()
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect();
  match run_child_process(&argv[0], &argv[1..], worktree_dir, &env_overrides) {
    Ok(status) => {
      if !status.success() {
        log_warn!("Bootstrap exited with status {}", status);
      }
    }
    Err(err) => {
      log_warn!("Bootstrap failed to start: {}", err);
    }
  }
}

fn create_files_symlink(paths: &crate::config::AgencyPaths, task: &TaskRef, worktree: &Path) {
  let files_dir = files_dir_for_task(paths, task);
  let local_path = local_files_path(worktree);

  if local_path.exists() || local_path.is_symlink() {
    return;
  }

  if let Some(parent) = local_path.parent()
    && let Err(err) = std::fs::create_dir_all(parent)
  {
    log_warn!("Could not create symlink parent: {err}");
    return;
  }

  #[cfg(unix)]
  {
    use std::os::unix::fs::symlink;
    if let Err(err) = symlink(&files_dir, &local_path) {
      log_warn!("Could not create symlink: {err}");
    }
  }

  #[cfg(windows)]
  {
    use std::os::windows::fs::symlink_dir;
    if let Err(err) = symlink_dir(&files_dir, &local_path) {
      log_warn!("Could not create symlink: {err}");
    }
  }
}

fn discover_root_entries(root_workdir: &Path) -> Result<Vec<fs::DirEntry>> {
  let mut out = Vec::new();
  for e in fs::read_dir(root_workdir)
    .with_context(|| format!("failed to read dir {}", root_workdir.display()))?
  {
    let e = e?;
    out.push(e);
  }
  Ok(out)
}

fn run_git_check_ignore_batch(root_workdir: &Path, rel_paths: &[&str]) -> Result<Vec<String>> {
  if rel_paths.is_empty() {
    return Ok(Vec::new());
  }
  let mut child = std::process::Command::new("git")
    .current_dir(root_workdir)
    .arg("check-ignore")
    .arg("--stdin")
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .spawn()
    .with_context(|| "failed to spawn git check-ignore --stdin")?;
  {
    use std::io::Write as _;
    let sin = child.stdin.as_mut().expect("piped stdin");
    for p in rel_paths {
      let _ = sin.write_all(p.as_bytes());
      let _ = sin.write_all(b"\n");
    }
  }
  let out = child
    .wait_with_output()
    .with_context(|| "failed to wait for git check-ignore")?;
  // exit 1 => no matches; treat as empty
  if !out.status.success() && out.status.code() != Some(1) {
    anyhow::bail!("git check-ignore failed: status={}", out.status);
  }
  let stdout = String::from_utf8_lossy(&out.stdout);
  Ok(
    stdout
      .lines()
      .filter(|l| !l.is_empty())
      .map(std::string::ToString::to_string)
      .collect(),
  )
}

// no per-file evaluator needed now; batched above

fn is_excluded(entry_name: &str, cfg: &BootstrapConfig) -> bool {
  matches!(entry_name, ".git" | ".agency") || cfg.exclude.iter().any(|e| e == entry_name)
}

fn copy_file(src: &Path, dst: &Path) -> Result<()> {
  if let Some(parent) = dst.parent() {
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
  }
  // Prefer reflink, fallback to regular copy automatically
  reflink_or_copy(src, dst)
    .map(|_| ())
    .with_context(|| format!("failed to copy {} -> {}", src.display(), dst.display()))
}

fn copy_dir_tree(src_dir: &Path, dst_dir: &Path) -> Result<()> {
  if dst_dir.exists() && !dst_dir.is_dir() {
    anyhow::bail!("destination exists and is not a dir: {}", dst_dir.display());
  }
  fs::create_dir_all(dst_dir).with_context(|| format!("failed to create {}", dst_dir.display()))?;
  for entry in fs::read_dir(src_dir).with_context(|| format!("read dir {}", src_dir.display()))? {
    let entry = entry?;
    let file_type = entry.file_type()?;
    let name = entry.file_name();
    let name = name.to_string_lossy().to_string();
    let src = entry.path();
    let dst = dst_dir.join(&name);
    if file_type.is_file() {
      if dst.exists() {
        continue;
      }
      if !file_size_within_limit(&src)? {
        continue;
      }
      // For included directories, copy regardless of ignore status within the dir
      copy_file(&src, &dst)?;
    } else if file_type.is_dir() {
      copy_dir_tree(&src, &dst)?;
    } else if file_type.is_symlink() {
      // Skip symlinks
      // no-op; fall through to next entry
    }
  }
  Ok(())
}

fn file_size_within_limit(path: &Path) -> Result<bool> {
  let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
  Ok(meta.len() <= MAX_BOOTSTRAP_FILE_BYTES)
}
