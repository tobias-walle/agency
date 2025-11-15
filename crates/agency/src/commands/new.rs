use std::fs;
use std::io::IsTerminal as _;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::log_info;
use crate::utils::daemon::notify_after_task_change;
use crate::utils::git::{current_branch_name, open_main_repo};
use crate::utils::log::t;
use crate::utils::task::{
  TaskContent, TaskFrontmatter, TaskRef, compute_unique_slug, edit_task_description, next_id,
  normalize_and_validate_slug, write_task_content,
};

pub fn run(
  ctx: &AppContext,
  slug: &str,
  agent: Option<&str>,
  desc: Option<&str>,
) -> Result<TaskRef> {
  notify_after_task_change(ctx, || {
    let base_slug = normalize_and_validate_slug(slug)?;

    let tasks = ctx.paths.tasks_dir();
    let _ = ensure_dir(&tasks)?;

    // Compute global next id and a unique slug
    let id = next_id(&tasks)?;
    let slug = compute_unique_slug(&tasks, &base_slug)?;

    // Determine base branch from current repo HEAD
    let repo = match open_main_repo(ctx.paths.cwd()) {
      Ok(r) => r,
      Err(_) => {
        bail!("Not in a git repository. Please run `git init` or cd to a repo.");
      }
    };
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

    // If description provided, write immediately and bypass editor
    if let Some(raw) = desc {
      content.body = raw.trim().to_string();
      write_task_content(&ctx.paths, &task, &content)?;
      log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));
    } else {
      let interactive = std::io::stdout().is_terminal();
      if interactive {
        // Open editor first; only write if content is non-empty
        match edit_task_description(
          &ctx.config,
          &ctx.paths,
          &task,
          ctx.paths.cwd(),
          &content.body,
        )? {
          Some(updated_body) => {
            content.body = updated_body;
            write_task_content(&ctx.paths, &task, &content)?;
            log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));
          }
          None => {
            // Do not create the task file; cancel
            bail!("Empty description");
          }
        }
      } else {
        // Non-interactive: create file immediately with empty body
        write_task_content(&ctx.paths, &task, &content)?;
        log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));
      }
    }

    // Worktree and bootstrap are now created lazily at attach time
    Ok(task)
  })
}

fn ensure_dir(dir: &Path) -> Result<bool> {
  if dir.exists() {
    return Ok(false);
  }
  fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
  Ok(true)
}

// editor helper now lives in utils::editor
