use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::utils::git::{ensure_branch_at, open_main_repo};
use crate::utils::task::{branch_name, parse_task_markdown, resolve_id_or_slug, task_file};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let task = resolve_id_or_slug(&ctx.paths, ident)?;

  // Resolve base from front matter or HEAD
  let tf_path = task_file(&ctx.paths, &task);
  let text = std::fs::read_to_string(&tf_path)
    .with_context(|| format!("failed to read {}", tf_path.display()))?;
  let (frontmatter, _body) = parse_task_markdown(&text);
  let base = frontmatter
    .and_then(|fm| fm.base_branch)
    .unwrap_or_else(|| crate::utils::git::head_branch(ctx));

  let repo = open_main_repo(ctx.paths.cwd())?;
  let branch = branch_name(&task);
  let _ = ensure_branch_at(&repo, &branch, &base)?;
  let _wt = crate::utils::bootstrap::prepare_worktree_for_task(ctx, &repo, &task, &branch)?;
  Ok(())
}
