use anyhow::{Context, Result, bail};

use crate::config::AgencyPaths;

use super::metadata::TaskRef;

pub fn resolve_id_or_slug(paths: &AgencyPaths, task_ident: &str) -> Result<TaskRef> {
  let tasks = paths.tasks_dir();
  if !tasks.exists() {
    bail!("tasks dir not found at {}", tasks.display());
  }
  if task_ident.chars().all(|c| c.is_ascii_digit()) {
    let id: u32 = task_ident.parse().context("invalid id")?;
    for entry in
      std::fs::read_dir(&tasks).with_context(|| format!("failed to read {}", tasks.display()))?
    {
      let path = entry?.path();
      if let Some(tf) = TaskRef::from_task_file(&path)
        && tf.id == id
      {
        return Ok(tf);
      }
    }
    bail!("task with id {task_ident} not found");
  }
  let slug = task_ident.to_string();
  for entry in
    std::fs::read_dir(&tasks).with_context(|| format!("failed to read {}", tasks.display()))?
  {
    let path = entry?.path();
    if let Some(tf) = TaskRef::from_task_file(&path)
      && tf.slug == slug
    {
      return Ok(tf);
    }
  }
  bail!("task with slug {task_ident} not found");
}

/// Resolve task identifier to a `TaskRef`, supporting `AGENCY_TASK_ID` env var fallback.
///
/// # Errors
/// Returns an error if the task cannot be resolved.
pub fn resolve_task_ident(paths: &AgencyPaths, task_ident: Option<&str>) -> Result<TaskRef> {
  if let Some(i) = task_ident {
    resolve_id_or_slug(paths, i)
  } else {
    let id_env = std::env::var("AGENCY_TASK_ID")
      .map_err(|_| anyhow::anyhow!("Not running in an agency environment. Cannot resolve task"))?;
    resolve_id_or_slug(paths, &id_env)
  }
}

pub fn list_tasks(paths: &AgencyPaths) -> Result<Vec<TaskRef>> {
  let tasks_dir = paths.tasks_dir();
  if !tasks_dir.exists() {
    return Ok(Vec::new());
  }
  let mut out = Vec::new();
  for entry in std::fs::read_dir(&tasks_dir)
    .with_context(|| format!("failed to read {}", tasks_dir.display()))?
  {
    let path = entry?.path();
    if let Some(tf) = TaskRef::from_task_file(&path) {
      out.push(tf);
    }
  }
  Ok(out)
}
