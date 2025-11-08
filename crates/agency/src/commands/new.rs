use std::fs;
use std::io::IsTerminal as _;
use std::path::Path;
use std::process::Command;

use anstream::println;
use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize as _;

use crate::config::AppContext;
use crate::utils::git::{add_worktree, ensure_branch, open_main_repo};
use crate::utils::task::{
  TaskFrontmatter, TaskRef, format_task_markdown, normalize_and_validate_slug,
};

pub fn run(ctx: &AppContext, slug: &str, no_edit: bool, agent: Option<&str>) -> Result<()> {
  let slug = normalize_and_validate_slug(slug)?;

  let tasks = ctx.paths.tasks_dir();
  let created = ensure_dir(&tasks)?;
  if created {
    println!("Created folder {}", ".agency/tasks".cyan());
  }

  if slug_exists(&tasks, &slug)? {
    bail!("Task with slug {slug} already exists");
  }

  let id = next_id(&tasks)?;
  let file_path = tasks.join(format!("{id}-{slug}.md"));
  // Optional YAML front matter
  let fm_opt = if let Some(agent_name) = agent {
    // Validate agent exists in config
    let _ = ctx.config.get_agent(agent_name)?;
    Some(TaskFrontmatter {
      agent: Some(agent_name.to_string()),
    })
  } else {
    None
  };
  let content = format_task_markdown(id, &slug, fm_opt.as_ref())?;
  fs::write(&file_path, content)
    .with_context(|| format!("failed to write {}", file_path.display()))?;

  // Git: open main repo, ensure branch, add worktree
  let repo = open_main_repo(ctx.paths.cwd())?;
  let branch_name = format!("agency/{id}-{slug}");
  let branch = ensure_branch(&repo, &branch_name)?;
  let branch_ref = branch.into_reference();
  let wt_name = format!("{id}-{slug}");
  let wt_root = ctx.paths.worktrees_dir();
  let _ = ensure_dir(&wt_root)?;
  let wt_dir = wt_root.join(&wt_name);
  add_worktree(&repo, &wt_name, &wt_dir, &branch_ref)?;

  println!("Task {} with id {} created ✨", slug.cyan(), id.cyan());

  // Optionally open the task file in the user's editor
  if std::io::stdout().is_terminal() && !no_edit {
    open_editor(&file_path)?;
  }

  Ok(())
}

fn ensure_dir(dir: &Path) -> Result<bool> {
  if dir.exists() {
    return Ok(false);
  }
  fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
  Ok(true)
}

fn slug_exists(tasks: &Path, slug: &str) -> Result<bool> {
  if !tasks.exists() {
    return Ok(false);
  }
  for entry in fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))? {
    let entry = entry?;
    let path = entry.path();
    if let Some(tf) = TaskRef::from_task_file(&path)
      && tf.slug == slug
    {
      return Ok(true);
    }
  }
  Ok(false)
}

fn next_id(tasks: &Path) -> Result<u32> {
  let mut max_id: u32 = 0;
  if tasks.exists() {
    for entry in
      fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))?
    {
      let entry = entry?;
      let path = entry.path();
      if let Some(tf) = TaskRef::from_task_file(&path)
        && tf.id > max_id
      {
        max_id = tf.id;
      }
    }
  }
  Ok(max_id.saturating_add(1))
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
