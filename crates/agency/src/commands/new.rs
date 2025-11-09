use std::fs;
use std::io::IsTerminal as _;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::AppContext;
use crate::log_info;
use crate::utils::daemon::notify_tasks_changed;
use crate::utils::git::{current_branch_name, open_main_repo};
use crate::utils::log::t;
use crate::utils::task::{
  TaskContent, TaskFrontmatter, TaskRef, compute_unique_slug, edit_task_description, next_id,
  normalize_and_validate_slug, write_task_content,
};

pub fn run(ctx: &AppContext, slug: &str, no_edit: bool, agent: Option<&str>) -> Result<TaskRef> {
  let base_slug = normalize_and_validate_slug(slug)?;

  let tasks = ctx.paths.tasks_dir();
  let _ = ensure_dir(&tasks)?;

  // Compute global next id and a unique slug
  let id = next_id(&tasks)?;
  let slug = compute_unique_slug(&tasks, &base_slug)?;

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

  let task = TaskRef {
    id,
    slug: slug.clone(),
  };
  let mut content = TaskContent {
    frontmatter: Some(fm),
    body: String::new(),
  };
  write_task_content(&ctx.paths, &task, &content)?;

  // Info messages with token highlights
  log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));

  // Optionally open the task file in the user's editor
  if std::io::stdout().is_terminal() && !no_edit {
    if let Some(updated_body) =
      edit_task_description(&ctx.paths, &task, ctx.paths.cwd(), &content.body)?
    {
      content.body = updated_body;
      write_task_content(&ctx.paths, &task, &content)?;
    }
  }

  // Worktree and bootstrap are now created lazily at attach time

  let _ = notify_tasks_changed(ctx);

  Ok(task)
}

fn ensure_dir(dir: &Path) -> Result<bool> {
  if dir.exists() {
    return Ok(false);
  }
  fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
  Ok(true)
}

// editor helper now lives in utils::editor
