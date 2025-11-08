use std::fs;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::utils::daemon::stop_sessions_of_task;
use crate::utils::git::{
  is_fast_forward, open_main_repo, rebase_onto, rev_parse, update_branch_ref,
};
use crate::utils::task::{
  branch_name, parse_task_markdown, resolve_id_or_slug, task_file, worktree_dir,
};

pub fn run(ctx: &AppContext, ident: &str, base_override: Option<&str>) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, ident)?;
  let branch = branch_name(&task);
  let wt_dir = worktree_dir(&ctx.paths, &task);
  let file_path = task_file(&ctx.paths, &task);
  if !file_path.exists() {
    bail!("task file not found: {}", file_path.display());
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

  anstream::println!(
    "{} {} -> {}",
    "Rebase".yellow(),
    branch.cyan(),
    base_branch.cyan()
  );

  // Rebase. On conflicts, instruct the user to resolve and rerun.
  if let Err(err) = rebase_onto(&wt_dir, &base_branch) {
    bail!(
      "rebase failed: {}. resolve conflicts in {} then rerun merge",
      err,
      wt_dir.display()
    );
  }

  // Fast-forward base to task head without switching HEAD.
  if !is_fast_forward(&repo, &base_branch, &branch)? {
    bail!("fast-forward not possible: base advanced; rerun after rebase");
  }
  let new_head = rev_parse(&wt_dir, "HEAD")?;
  anstream::println!(
    "{} {} to {} at {}",
    "Fast-forward".green(),
    base_branch.cyan(),
    branch.cyan(),
    new_head.cyan()
  );
  update_branch_ref(&repo, &base_branch, &new_head)?;

  // Stop any running sessions for this task (best-effort)
  let _ = stop_sessions_of_task(ctx, &task);

  // Cleanup: worktree, branch, task file
  anstream::println!("{}", "Cleaning up: worktree, branch, file".yellow());
  {
    use crate::utils::git::{delete_branch_if_exists, prune_worktree_if_exists};
    let _ = prune_worktree_if_exists(&repo, &wt_dir)?;
    let _ = delete_branch_if_exists(&repo, &branch)?;
  }
  if file_path.exists() {
    fs::remove_file(&file_path)
      .with_context(|| format!("failed to remove {}", file_path.display()))?;
  }
  anstream::println!("{}", "Merge complete and cleaned up".green());

  Ok(())
}
