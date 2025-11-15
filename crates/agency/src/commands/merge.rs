use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::utils::daemon::{notify_after_task_change, stop_sessions_of_task};
use crate::utils::git::{
  current_branch_name_at, git_workdir, hard_reset_to_head_at, is_fast_forward_at, rebase_onto,
  rev_parse, stash_pop, stash_push, update_branch_ref_at, worktree_is_clean_at,
};
use crate::utils::task::{
  branch_name, parse_task_markdown, resolve_id_or_slug, task_file, worktree_dir,
};
use crate::{log_success, log_warn};

pub fn run(ctx: &AppContext, ident: &str, base_override: Option<&str>) -> Result<()> {
  notify_after_task_change(ctx, || {
    let inputs = compute_merge_inputs(ctx, ident, base_override)?;

    let (refresh_checked_out_base, needs_auto_stash) =
      assess_base_state(&inputs.repo_workdir, &inputs.base_branch)?;

    log_warn!("Rebase {} onto {}", inputs.branch, inputs.base_branch);
    perform_rebase(&inputs.wt_dir, &inputs.base_branch)?;

    let (_base_head, new_head) = ensure_can_fast_forward(
      &inputs.repo_workdir,
      &inputs.wt_dir,
      &inputs.base_branch,
      &inputs.branch,
    )?;

    let mut pending_stash =
      maybe_autostash(&inputs.repo_workdir, &inputs.base_branch, needs_auto_stash)?;

    log_success!(
      "Fast-forward {} to {} at {}",
      inputs.base_branch,
      inputs.branch,
      new_head
    );
    update_branch_ref_at(&inputs.repo_workdir, &inputs.base_branch, &new_head)?;

    fast_forward_refresh_and_unstash(
      &inputs.repo_workdir,
      &inputs.base_branch,
      refresh_checked_out_base,
      &mut pending_stash,
    )?;

    // Stop any running sessions for this task (best-effort)
    let _ = stop_sessions_of_task(ctx, &inputs.task);

    cleanup_task_artifacts(
      &inputs.repo_workdir,
      &inputs.wt_dir,
      &inputs.branch,
      &inputs.file_path,
    )?;

    log_success!("Merge complete");

    Ok(())
  })
}

struct MergeInputs {
  task: crate::utils::task::TaskRef,
  branch: String,
  wt_dir: PathBuf,
  file_path: PathBuf,
  base_branch: String,
  repo_workdir: PathBuf,
}

fn compute_merge_inputs(
  ctx: &AppContext,
  ident: &str,
  base_override: Option<&str>,
) -> Result<MergeInputs> {
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let branch = branch_name(&task);
  let wt_dir = worktree_dir(&ctx.paths, &task);
  let file_path = task_file(&ctx.paths, &task);
  if !file_path.exists() {
    bail!("Task file not found: {}", file_path.display());
  }
  let data = fs::read_to_string(&file_path)
    .with_context(|| format!("failed to read {}", file_path.display()))?;
  let (fm_opt, _body) = parse_task_markdown(&data);
  let mut base_branch = fm_opt
    .and_then(|fm| fm.base_branch)
    .unwrap_or_else(|| "main".to_string());
  if let Some(b) = base_override {
    base_branch = b.to_string();
  }
  let repo_workdir = git_workdir(ctx.paths.cwd())?;
  Ok(MergeInputs {
    task,
    branch,
    wt_dir,
    file_path,
    base_branch,
    repo_workdir,
  })
}

fn assess_base_state(repo_workdir: &Path, base_branch: &str) -> Result<(bool, bool)> {
  let mut refresh_checked_out_base = false;
  let mut needs_auto_stash = false;
  if let Ok(Some(cur)) = current_branch_name_at(repo_workdir)
    && cur == base_branch
  {
    refresh_checked_out_base = true;
    if worktree_is_clean_at(repo_workdir)? {
      log_warn!(
        "Base is checked out and clean; will refresh after merge: {}",
        base_branch
      );
    } else {
      needs_auto_stash = true;
      log_warn!(
        "Base is checked out with changes; will auto-stash before merge: {}",
        base_branch
      );
    }
  }
  Ok((refresh_checked_out_base, needs_auto_stash))
}

fn perform_rebase(wt_dir: &Path, base_branch: &str) -> Result<()> {
  if let Err(err) = rebase_onto(wt_dir, base_branch) {
    bail!(
      "Rebase failed: {}. Resolve conflicts in {} then rerun merge",
      err,
      wt_dir.display()
    );
  }
  Ok(())
}

fn ensure_can_fast_forward(
  repo_workdir: &Path,
  wt_dir: &Path,
  base_branch: &str,
  branch: &str,
) -> Result<(String, String)> {
  let base_head = rev_parse(repo_workdir, base_branch)?;
  let new_head = rev_parse(wt_dir, "HEAD")?;
  if base_head == new_head {
    bail!("No changes to merge: {base_branch} already at {new_head}");
  }
  if !is_fast_forward_at(repo_workdir, base_branch, branch)? {
    bail!("Fast-forward not possible: Base advanced; rerun after rebase");
  }
  Ok((base_head, new_head))
}

fn maybe_autostash(
  repo_workdir: &Path,
  base_branch: &str,
  needs_auto_stash: bool,
) -> Result<Option<AutoStash>> {
  if !needs_auto_stash {
    return Ok(None);
  }
  let message = format!("agency auto-stash before merge {base_branch}");
  let result = stash_push(repo_workdir, &message)?;
  let stash = match result {
    Some(stash_ref) => {
      log_warn!(
        "Auto-stashed checked-out base before fast-forward: {}",
        base_branch
      );
      Some(AutoStash::new(repo_workdir, stash_ref))
    }
    None => {
      log_warn!(
        "Base reported dirty but nothing to stash; continuing merge: {}",
        base_branch
      );
      None
    }
  };
  Ok(stash)
}

fn fast_forward_refresh_and_unstash(
  repo_workdir: &Path,
  base_branch: &str,
  refresh_checked_out_base: bool,
  pending_stash: &mut Option<AutoStash>,
) -> Result<()> {
  if !refresh_checked_out_base {
    return Ok(());
  }
  hard_reset_to_head_at(repo_workdir)?;
  log_success!("Refreshed checked-out working tree for {}", base_branch);
  if let Some(mut stash) = pending_stash.take() {
    let stash_ref = stash.stash_ref.clone();
    if let Err(err) = stash.pop() {
      stash.abandon();
      bail!(
        "Auto-stash {stash_ref} failed to reapply: {err}. Resolve manually with `git stash pop {stash_ref}` then rerun merge"
      );
    }
    log_success!("Restored auto-stashed changes for {}", base_branch);
  }
  Ok(())
}

fn cleanup_task_artifacts(
  repo_workdir: &Path,
  wt_dir: &Path,
  branch: &str,
  file_path: &Path,
) -> Result<()> {
  use crate::utils::git::{delete_branch_if_exists_at, prune_worktree_if_exists_at};
  let _ = prune_worktree_if_exists_at(repo_workdir, wt_dir)?;
  let _ = delete_branch_if_exists_at(repo_workdir, branch)?;
  if file_path.exists() {
    fs::remove_file(file_path)
      .with_context(|| format!("failed to remove {}", file_path.display()))?;
  }
  Ok(())
}

struct AutoStash {
  workdir: PathBuf,
  stash_ref: String,
  active: bool,
}

impl AutoStash {
  fn new(workdir: &Path, stash_ref: String) -> Self {
    Self {
      workdir: workdir.to_path_buf(),
      stash_ref,
      active: true,
    }
  }

  fn pop(&mut self) -> Result<()> {
    stash_pop(&self.workdir, &self.stash_ref)?;
    self.active = false;
    Ok(())
  }

  fn abandon(&mut self) {
    self.active = false;
  }
}

impl Drop for AutoStash {
  fn drop(&mut self) {
    if self.active {
      let _ = stash_pop(&self.workdir, &self.stash_ref);
    }
  }
}
