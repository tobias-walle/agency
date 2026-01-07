use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{AgencyConfig, AgencyPaths};
use crate::utils::editor::open_path as open_editor;

use super::metadata::{TaskContent, TaskFrontmatter, TaskRef};
use super::paths::task_file;

const FRONT_MATTER_START: &str = "---\n";
const FRONT_MATTER_END: &str = "\n---\n";

/// Parse optional YAML front matter from the start of a task markdown string.
/// Returns the parsed front matter (if present) and a slice of the body
/// excluding the front matter. Gracefully ignores malformed boundaries.
pub fn parse_task_markdown(input: &str) -> (Option<TaskFrontmatter>, &str) {
  // Expect YAML front matter delimited by FRONT_MATTER_START and FRONT_MATTER_END.
  if let Some(rest) = input.strip_prefix(FRONT_MATTER_START)
    && let Some((yaml, body)) = rest.split_once(FRONT_MATTER_END)
  {
    let fm = serde_yaml::from_str::<TaskFrontmatter>(yaml).ok();
    return (fm, body);
  }
  (None, input)
}

pub fn read_task_content(paths: &AgencyPaths, task: &TaskRef) -> Result<TaskContent> {
  let tf = task_file(paths, task);
  let data =
    std::fs::read_to_string(&tf).with_context(|| format!("failed to read {}", tf.display()))?;
  let (frontmatter, body) = parse_task_markdown(&data);
  Ok(TaskContent {
    frontmatter,
    body: body.to_string(),
  })
}

pub fn write_task_content(
  paths: &AgencyPaths,
  task: &TaskRef,
  content: &TaskContent,
) -> Result<()> {
  let tf = task_file(paths, task);
  if let Some(dir) = tf.parent() {
    std::fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
  }

  let mut output = String::new();
  if let Some(fm) = &content.frontmatter {
    let yaml = serde_yaml::to_string(fm).context("failed to serialize front matter")?;
    output.push_str(FRONT_MATTER_START);
    output.push_str(yaml.trim());
    output.push_str(FRONT_MATTER_END);
  }

  if !content.body.is_empty() {
    output.push_str(&content.body);
    if !content.body.ends_with('\n') {
      output.push('\n');
    }
  }

  std::fs::write(&tf, output).with_context(|| format!("failed to write {}", tf.display()))?;
  Ok(())
}

pub fn edit_task_description(
  cfg: &AgencyConfig,
  paths: &AgencyPaths,
  task: &TaskRef,
  project_root: &Path,
  initial_body: &str,
) -> Result<Option<String>> {
  let state_dir = paths.state_dir();
  std::fs::create_dir_all(&state_dir)
    .with_context(|| format!("failed to create {}", state_dir.display()))?;

  let temp_path = state_dir.join(format!("{}-{}.desc.md", task.id, task.slug));
  std::fs::write(&temp_path, initial_body)
    .with_context(|| format!("failed to write {}", temp_path.display()))?;

  let editor_result = open_editor(cfg, &temp_path, project_root);
  let updated = std::fs::read_to_string(&temp_path)
    .with_context(|| format!("failed to read {}", temp_path.display()));
  let _ = std::fs::remove_file(&temp_path);

  editor_result?;
  let updated = updated?;
  if updated.trim().is_empty() {
    return Ok(None);
  }

  Ok(Some(updated))
}

/// Read and parse a task's front matter from disk, if present.
pub fn read_task_frontmatter(paths: &AgencyPaths, task: &TaskRef) -> Option<TaskFrontmatter> {
  let tf = task_file(paths, task);
  let Ok(text) = std::fs::read_to_string(&tf) else {
    return None;
  };
  let (fm, _body) = parse_task_markdown(&text);
  fm
}
