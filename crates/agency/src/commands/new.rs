use std::fs;
use std::path::Path;

use anstream::println;
use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize as _;

use crate::config::AgencyConfig;
use crate::utils::git::{add_worktree, ensure_branch, open_main_repo};
use crate::utils::task::{TaskRef, normalize_and_validate_slug};

pub fn run(cfg: &AgencyConfig, slug: &str) -> Result<()> {
  let slug = normalize_and_validate_slug(slug)?;

  let tasks = cfg.tasks_dir();
  let created = ensure_dir(&tasks)?;
  if created {
    println!("Created folder {}", ".agency/tasks".cyan());
  }

  if slug_exists(&tasks, &slug)? {
    bail!("Task with slug {} already exists", slug);
  }

  let id = next_id(&tasks)?;
  let file_path = tasks.join(format!("{}-{}.md", id, slug));
  let content = format!("# Task {}: {}\n", id, slug);
  fs::write(&file_path, content)
    .with_context(|| format!("failed to write {}", file_path.display()))?;

  // Git: open main repo, ensure branch, add worktree
  let repo = open_main_repo(cfg.cwd())?;
  let branch_name = format!("agency/{}-{}", id, slug);
  let branch = ensure_branch(&repo, &branch_name)?;
  let branch_ref = branch.into_reference();
  let wt_name = format!("{}-{}", id, slug);
  let wt_root = cfg.worktrees_dir();
  let _ = ensure_dir(&wt_root)?;
  let wt_dir = wt_root.join(&wt_name);
  add_worktree(&repo, &wt_name, &wt_dir, &branch_ref)?;

  println!("Task {} with id {} created ✨", slug.cyan(), id.cyan());

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
