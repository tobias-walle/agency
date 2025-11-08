use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reflink_copy::reflink_or_copy;

use crate::config::BootstrapConfig;
use crate::utils::cmd::{CmdCtx, expand_argv};
use gix as git;

const MAX_BOOTSTRAP_FILE_BYTES: u64 = 10 * 1024 * 1024;

pub fn bootstrap_worktree(
  repo: &git::Repository,
  root_workdir: &Path,
  dst_worktree: &Path,
  cfg: &BootstrapConfig,
) -> Result<()> {
  let entries = discover_root_entries(root_workdir)?;

  // Split entries by type and filter out excluded names up front
  let mut file_names: Vec<String> = Vec::new();
  let mut file_paths: Vec<std::path::PathBuf> = Vec::new();
  let mut dir_entries: Vec<(String, std::path::PathBuf)> = Vec::new();
  for entry in entries {
    let Ok(file_type) = entry.file_type() else {
      continue;
    };
    let name = entry.file_name().to_string_lossy().to_string();
    if is_excluded(&name, cfg) {
      continue;
    }
    let path = entry.path();
    if file_type.is_file() {
      file_names.push(name);
      file_paths.push(path);
    } else if file_type.is_dir() {
      dir_entries.push((name, path));
    }
  }

  // Batch evaluate ignore status for root files once
  let ignored = run_git_check_ignore_batch(
    repo.workdir().unwrap_or(root_workdir),
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
    if !file_size_within_limit(src_path)? {
      continue;
    }
    let dst_path = dst_worktree.join(name);
    if dst_path.exists() {
      continue;
    }
    copy_file(src_path, &dst_path)?;
  }

  for (name, src_dir) in dir_entries {
    if !cfg.include.iter().any(|i| i == &name) {
      continue;
    }
    let dst_dir = dst_worktree.join(&name);
    copy_dir_tree(&src_dir, &dst_dir)?;
  }

  Ok(())
}

/// Run the configured bootstrap command inside the new worktree.
///
/// - Replaces `<root>` placeholders in argv with the repository root path.
/// - If `cfg.cmd` equals the default and the file does not exist, it silently skips.
/// - Streams child stdout/stderr directly to the user.
pub fn run_bootstrap_cmd(repo_root: &Path, worktree_dir: &Path, cfg: &BootstrapConfig) {
  // No command configured -> no-op
  if cfg.cmd.is_empty() {
    return;
  }

  // Build expansion context and expand argv
  let root_abs = repo_root
    .canonicalize()
    .unwrap_or_else(|_| repo_root.to_path_buf())
    .display()
    .to_string();
  let ctx = CmdCtx::from_process_env(root_abs.clone());
  let argv = expand_argv(&cfg.cmd, &ctx);

  // Special-case: default path missing should be a silent skip
  if cfg.cmd.len() == 1 && cfg.cmd[0] == "<root>/.agency/setup.sh" {
    let candidate = PathBuf::from(&argv[0]);
    if !candidate.exists() {
      return;
    }
  }

  // Spawn the command, current_dir at the worktree
  let _ = std::process::Command::new(&argv[0])
    .current_dir(worktree_dir)
    .args(&argv[1..])
    .stdin(std::process::Stdio::inherit())
    .stdout(std::process::Stdio::inherit())
    .stderr(std::process::Stdio::inherit())
    .status();
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
