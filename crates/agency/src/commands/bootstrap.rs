use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::utils::bootstrap::{create_worktree_for_task, run_bootstrap_cmd_with_env};
use crate::utils::git::{ensure_branch_at, open_main_repo, repo_workdir_or, rev_parse};
use crate::utils::task::{
  TaskFrontmatterExt, branch_name, parse_task_markdown, resolve_id_or_slug, task_file,
};

/// User-facing bootstrap: prepares worktree and runs bootstrap for a task.
pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, ident)?;

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

  // Create worktree and copy bootstrap files
  let wt_result = create_worktree_for_task(ctx, &repo, &task, &branch)?;

  // Run bootstrap command synchronously
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let bcfg = ctx.config.bootstrap_config();
  let env_vars: HashMap<String, String> = std::env::vars().collect();
  run_bootstrap_cmd_with_env(&repo_root, &wt_result.worktree_dir, &bcfg, &env_vars);

  Ok(())
}
