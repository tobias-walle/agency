use std::fs;

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::utils::daemon::stop_sessions_of_task;
use crate::utils::git::{
  current_branch_name, hard_reset_to_head, is_fast_forward, open_main_repo, rebase_onto, rev_parse,
  update_branch_ref, worktree_is_clean,
};
use crate::utils::task::{
  branch_name, parse_task_markdown, resolve_id_or_slug, task_file, worktree_dir,
};
use crate::{log_success, log_warn};

pub fn run(ctx: &AppContext, ident: &str, base_override: Option<&str>) -> Result<()> {
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

  // Open main repo once
  let repo = open_main_repo(ctx.paths.cwd())?;

  // If the base branch is currently checked out in the main worktree,
  // ensure it is clean to avoid leaving the working tree in a dirty state.
  let mut refresh_checked_out_base = false;
  if let Ok(cur) = current_branch_name(&repo)
    && cur == base_branch
  {
    if !worktree_is_clean(&repo)? {
      bail!(
        "Base branch {base_branch} is checked out and has uncommitted changes; commit or stash before merging"
      );
    }
    refresh_checked_out_base = true;
    log_warn!(
      "Base is checked out and clean; will refresh after merge: {}",
      base_branch
    );
  }

  log_warn!("Rebase {} onto {}", branch, base_branch);

  // Rebase. On conflicts, instruct the user to resolve and rerun.
  if let Err(err) = rebase_onto(&wt_dir, &base_branch) {
    bail!(
      "Rebase failed: {}. Resolve conflicts in {} then rerun merge",
      err,
      wt_dir.display()
    );
  }

  // Fast-forward base to task head without switching HEAD.
  if !is_fast_forward(&repo, &base_branch, &branch)? {
    bail!("Fast-forward not possible: Base advanced; rerun after rebase");
  }
  let new_head = rev_parse(&wt_dir, "HEAD")?;
  log_success!("Fast-forward {} to {} at {}", base_branch, branch, new_head);
  update_branch_ref(&repo, &base_branch, &new_head)?;
  if refresh_checked_out_base {
    hard_reset_to_head(&repo)?;
    log_success!("Refreshed checked-out working tree for {}", base_branch);
  }

  // Stop any running sessions for this task (best-effort)
  let _ = stop_sessions_of_task(ctx, &task);

  // Cleanup: worktree, branch, task file
  log_warn!("Clean up: worktree, branch, file");
  {
    use crate::utils::git::{delete_branch_if_exists, prune_worktree_if_exists};
    let _ = prune_worktree_if_exists(&repo, &wt_dir)?;
    let _ = delete_branch_if_exists(&repo, &branch)?;
  }
  if file_path.exists() {
    fs::remove_file(&file_path)
      .with_context(|| format!("failed to remove {}", file_path.display()))?;
  }
  log_success!("Merge complete");

  Ok(())
}
