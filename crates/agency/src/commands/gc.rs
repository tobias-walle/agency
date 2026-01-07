use std::collections::HashSet;

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::utils::error_messages;
use crate::utils::git::{delete_branch_if_exists, open_main_repo, prune_worktree_if_exists};
use crate::utils::log::t;
use crate::utils::task::list_tasks;
use crate::{log_info, log_success, log_warn};

fn list_agency_branches(repo: &gix::Repository) -> Result<Vec<String>> {
  // List all refs under refs/heads/agency/* and return short names "<id>-<slug>"
  let workdir = repo
    .workdir()
    .ok_or_else(|| anyhow::anyhow!("no main worktree: cannot list branches"))?;
  let out = std::process::Command::new("git")
    .current_dir(workdir)
    .args(["for-each-ref", "--format=%(refname)", "refs/heads/agency"]) // prefix match
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .output()
    .with_context(|| "failed to run git for-each-ref")?;
  if !out.status.success() {
    bail!(error_messages::git_command_failed(
      "for-each-ref",
      out.status
    ));
  }
  let stdout = String::from_utf8_lossy(&out.stdout);
  let mut names = Vec::new();
  for line in stdout.lines() {
    if let Some(short) = line.strip_prefix("refs/heads/agency/") {
      names.push(short.to_string());
    }
  }
  Ok(names)
}

pub fn run(ctx: &AppContext) -> Result<()> {
  let repo = open_main_repo(ctx.paths.root())?;

  // Build set of valid task keys: "<id>-<slug>"
  let valid: HashSet<String> = list_tasks(&ctx.paths)?
    .into_iter()
    .map(|t| format!("{}-{}", t.id, t.slug))
    .collect();
  log_info!("Found {} valid tasks", valid.len());

  // Sweep worktrees under .agency/worktrees/* first
  let wt_root = ctx.paths.worktrees_dir();
  let mut pruned_worktrees = 0usize;
  if wt_root.exists() {
    for entry in std::fs::read_dir(&wt_root)
      .with_context(|| format!("failed to read {}", wt_root.display()))?
    {
      let path = entry?.path();
      if path.is_dir()
        && let Some(name) = path.file_name().and_then(|n| n.to_str())
        && !valid.contains(name)
      {
        if prune_worktree_if_exists(&repo, &path)? {
          pruned_worktrees += 1;
          log_success!("Pruned worktree {}", t::path(path.display()));
        } else {
          log_warn!(
            "Worktree not linked or already removed {}",
            t::path(path.display())
          );
        }
      }
    }
  }

  // Sweep branches under refs/heads/agency/*
  // Safety: Only delete branches with no task AND no worktree dir.
  let mut deleted_branches = 0usize;
  for short in list_agency_branches(&repo)? {
    if !valid.contains(&short) {
      let wt_dir_for_branch = wt_root.join(&short);
      if wt_dir_for_branch.exists() {
        log_warn!(
          "Skip branch without task due to existing worktree {}",
          t::path(wt_dir_for_branch.display())
        );
      } else {
        let full = format!("agency/{short}");
        if delete_branch_if_exists(&repo, &full)? {
          deleted_branches += 1;
          log_success!("Deleted branch {}", full);
        }
      }
    }
  }

  log_success!(
    "Garbage collected {} branches, {} worktrees",
    deleted_branches,
    pruned_worktrees
  );
  Ok(())
}
