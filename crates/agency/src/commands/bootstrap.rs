use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{AppContext, BootstrapConfig};
use crate::utils::bootstrap::{create_worktree_for_task, run_bootstrap_in_worktree};
use crate::utils::git::{ensure_branch_at, open_main_repo, repo_workdir_or, rev_parse};
use crate::utils::task::{
  TaskFrontmatterExt, branch_name, parse_task_markdown, resolve_id_or_slug, task_file,
};

/// User-facing bootstrap: prepares worktree and runs bootstrap for a task.
pub fn run(ctx: &AppContext, task_ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, task_ident)?;

  // Resolve base from front matter or HEAD
  let tf_path = task_file(&ctx.paths, &task);
  let text = std::fs::read_to_string(&tf_path)
    .with_context(|| format!("failed to read {}", tf_path.display()))?;
  let (frontmatter, _body) = parse_task_markdown(&text);
  let base = frontmatter.base_branch(ctx);

  let repo = open_main_repo(ctx.paths.root())?;
  let branch = branch_name(&task);
  // Ensure base branch resolves to a commit; provide friendly guidance when unborn
  if rev_parse(repo.workdir().unwrap_or(ctx.paths.root()), &base).is_err() {
    anyhow::bail!(
      "No worktree can be created as base branch has no commits. Please create an initial commit in your basebranch, e.g. by using `touch README.md; git add .; git commit -m 'init'`."
    );
  }
  let _ = ensure_branch_at(&repo, &branch, &base)?;

  // Create worktree (fast)
  let wt_result = create_worktree_for_task(ctx, &repo, &task, &branch)?;

  // Run bootstrap synchronously (this is the user-facing command)
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let bcfg = ctx.config.bootstrap_config();
  let env_vars: HashMap<String, String> = std::env::vars().collect();
  run_bootstrap_in_worktree(&repo_root, &wt_result.worktree_dir, &bcfg, &env_vars)?;

  Ok(())
}

/// Internal bootstrap: run by daemon as a child process.
/// This function is called with pre-computed paths and config.
pub fn run_internal(
  repo_root: &str,
  worktree_dir: &str,
  include: &[String],
  exclude: &[String],
  cmd: &[String],
) -> Result<()> {
  let repo_root = Path::new(repo_root);
  let worktree_dir = Path::new(worktree_dir);

  let bcfg = BootstrapConfig {
    include: include.to_vec(),
    exclude: exclude.to_vec(),
    cmd: cmd.to_vec(),
  };

  // Collect environment variables passed by the daemon
  let env_vars: HashMap<String, String> = std::env::vars().collect();

  run_bootstrap_in_worktree(repo_root, worktree_dir, &bcfg, &env_vars)?;

  Ok(())
}
