use std::fs;
use std::io::IsTerminal as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::log_info;
use crate::utils::daemon::notify_after_task_change;
use crate::utils::files::add_file;
use crate::utils::git::current_branch_name_at;
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
  edit: bool,
  files: &[String],
) -> Result<TaskRef> {
  notify_after_task_change(ctx, || {
    let base_slug = normalize_and_validate_slug(slug)?;

    let tasks = ctx.paths.tasks_dir();
    let _ = ensure_dir(&tasks)?;

    // Compute global next id and a unique slug
    let id = next_id(&tasks)?;
    let slug = compute_unique_slug(&tasks, &base_slug)?;

    // Determine base branch from current working directory
    let base_branch = match current_branch_name_at(ctx.paths.cwd()) {
      Ok(Some(name)) => name,
      Ok(None) => "main".to_string(),
      Err(_) => {
        bail!("Not in a git repository. Please run `git init` or cd to a repo.");
      }
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

    let should_open_editor = edit || desc.is_none();
    if should_open_editor {
      let interactive = std::io::stdout().is_terminal();
      if interactive {
        let initial = desc.map(str::trim).unwrap_or_default();
        content.body = initial.to_string();
        match edit_task_description(
          &ctx.config,
          &ctx.paths,
          &task,
          ctx.paths.root(),
          &content.body,
        )? {
          Some(updated_body) => {
            content.body = updated_body;
            write_task_content(&ctx.paths, &task, &content)?;
            log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));
          }
          None => {
            bail!("Empty description");
          }
        }
      } else {
        content.body = desc.map(|d| d.trim().to_string()).unwrap_or_default();
        write_task_content(&ctx.paths, &task, &content)?;
        log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));
      }
    } else {
      content.body = desc
        .expect("desc must be Some when not opening editor")
        .trim()
        .to_string();
      write_task_content(&ctx.paths, &task, &content)?;
      log_info!("Create task {} (id {})", t::slug(&slug), t::id(id));
    }

    for file_path in files {
      let path = PathBuf::from(file_path);
      match add_file(&ctx.paths, &task, &path) {
        Ok(file_ref) => {
          log_info!("Attached file {} {}", t::id(file_ref.id), t::path(&file_ref.name));
        }
        Err(err) => {
          crate::log_warn!("Failed to attach {}: {}", file_path, err);
        }
      }
    }

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
