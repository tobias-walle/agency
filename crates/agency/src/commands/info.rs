use anyhow::Result;

use crate::config::AppContext;
use crate::utils::context::{detect_task_from_env, is_in_worktree};
use crate::utils::files::{list_files, print_files_table};
use crate::utils::task::{
  TaskFrontmatterExt, agent_for_task, read_task_frontmatter, worktree_dir,
};

pub fn run(ctx: &AppContext) -> Result<()> {
  let task = detect_task_from_env(&ctx.paths)?;
  let frontmatter = read_task_frontmatter(&ctx.paths, &task);
  let base_branch = frontmatter.base_branch(ctx);
  let agent_name = agent_for_task(&ctx.config, frontmatter.as_ref())
    .unwrap_or_else(|| "(not set)".to_string());
  let wt_dir = worktree_dir(&ctx.paths, &task);

  println!("Task: {}-{}", task.id, task.slug);
  println!("Base: {base_branch}");
  println!("Agent: {agent_name}");
  println!("Worktree: {}", wt_dir.display());
  println!();

  let files = list_files(&ctx.paths, &task)?;
  println!("Files:");

  if files.is_empty() {
    println!("  No files attached.");
  } else {
    let in_worktree = is_in_worktree(&ctx.paths);
    print_files_table(&ctx.paths, &task, &files, in_worktree);
  }

  println!();
  println!("Note: Do not reference agency task ID or slug in commit messages.");

  Ok(())
}
