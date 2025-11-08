use std::fs;
use std::io::IsTerminal as _;
use std::path::Path;
use std::process::Command;

use anstream::println;
use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::utils::bootstrap::{bootstrap_worktree, run_bootstrap_cmd};
use crate::utils::git::{
  add_worktree_for_branch, current_branch_name, ensure_branch, open_main_repo, repo_workdir_or,
};
use crate::utils::task::{
  TaskFrontmatter, TaskRef, compute_unique_slug, format_task_markdown, next_id,
  normalize_and_validate_slug,
};

pub fn run(ctx: &AppContext, slug: &str, no_edit: bool, agent: Option<&str>) -> Result<TaskRef> {
  let base_slug = normalize_and_validate_slug(slug)?;

  let tasks = ctx.paths.tasks_dir();
  let _ = ensure_dir(&tasks)?;

  // Compute global next id and a unique slug
  let id = next_id(&tasks)?;
  let slug = compute_unique_slug(&tasks, &base_slug)?;

  let file_path = tasks.join(format!("{id}-{slug}.md"));

  // Determine base branch from current repo HEAD
  let repo = open_main_repo(ctx.paths.cwd())?;
  let base_branch = match current_branch_name(&repo) {
    Ok(name) => name,
    Err(_) => "main".to_string(),
  };

  // Compose YAML front matter
  let fm = if let Some(agent_name) = agent {
    // Validate agent exists in config
    let _ = ctx.config.get_agent(agent_name)?;
    TaskFrontmatter {
      agent: Some(agent_name.to_string()),
      base_branch: Some(base_branch),
    }
  } else {
    TaskFrontmatter {
      agent: None,
      base_branch: Some(base_branch),
    }
  };
  let content = format_task_markdown(&slug, Some(&fm))?;
  fs::write(&file_path, content)
    .with_context(|| format!("failed to write {}", file_path.display()))?;

  // Git: open main repo, ensure branch, add worktree
  let branch_name = format!("agency/{id}-{slug}");
  let _ = ensure_branch(&repo, &branch_name)?;
  let wt_name = format!("{id}-{slug}");
  let wt_root = ctx.paths.worktrees_dir();
  let _ = ensure_dir(&wt_root)?;
  let wt_dir = wt_root.join(&wt_name);
  // First concise task creation log
  println!("Create Task {} with id {}", slug.cyan(), id.cyan());
  // Bootstrap worktree log
  println!(
    "Bootstrap worktree {} ...",
    format!("agency/{}-{}", id, slug).cyan()
  );
  add_worktree_for_branch(&repo, &wt_name, &wt_dir, &branch_name)?;

  // Optionally open the task file in the user's editor
  if std::io::stdout().is_terminal() && !no_edit {
    open_editor(&file_path)?;
  }

  // Bootstrap git-ignored root files into the new worktree AFTER editing for better UX
  let root_workdir = repo_workdir_or(&repo, ctx.paths.cwd());
  let bcfg = ctx.config.bootstrap_config();
  bootstrap_worktree(&repo, &root_workdir, &wt_dir, &bcfg)?;
  // Run optional bootstrap command within the new worktree
  let _ = run_bootstrap_cmd(&root_workdir, &wt_dir, &bcfg);

  Ok(TaskRef { id, slug })
}

fn ensure_dir(dir: &Path) -> Result<bool> {
  if dir.exists() {
    return Ok(false);
  }
  fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
  Ok(true)
}

fn open_editor(path: &Path) -> Result<()> {
  // Resolve editor from $EDITOR or fallback to vi
  let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
  let file = path
    .canonicalize()
    .unwrap_or_else(|_| path.to_path_buf())
    .display()
    .to_string();

  // Parse $EDITOR into program and args using shell-words
  let tokens = shell_words::split(&editor).context("invalid $EDITOR value")?;
  if tokens.is_empty() {
    bail!("invalid $EDITOR value: empty");
  }
  let program = &tokens[0];
  let mut args: Vec<String> = tokens[1..].to_vec();
  args.push(file);

  // Spawn editor directly, inheriting env vars
  let status = Command::new(program)
    .args(&args)
    .status()
    .with_context(|| format!("failed to spawn editor program: {program}"))?;

  if !status.success() {
    bail!("editor exited with non-zero status");
  }
  Ok(())
}
