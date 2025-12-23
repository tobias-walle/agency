use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::utils::git::{ensure_branch_at, open_main_repo, rev_parse};
use crate::utils::task::{
  TaskFrontmatterExt, branch_name, parse_task_markdown, resolve_id_or_slug, task_file,
};

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
  let _wt = crate::utils::bootstrap::prepare_worktree_for_task(ctx, &repo, &task, &branch)?;
  Ok(())
}
