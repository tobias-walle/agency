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

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use tempfile::TempDir;

  #[test]
  fn parse_task_markdown_with_empty_frontmatter() {
    let input = "---\n{}\n---\n\nBody text";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_some());
    assert!(body.contains("Body text"));
  }

  #[test]
  fn parse_task_markdown_with_base_branch() {
    let input = "---\nbase_branch: develop\n---\n\nContent";
    let (fm, body) = parse_task_markdown(input);
    let fm = fm.expect("should have frontmatter");
    assert_eq!(fm.base_branch.as_deref(), Some("develop"));
    assert!(body.contains("Content"));
  }

  #[test]
  fn parse_task_markdown_with_both_fields() {
    let input = "---\nagent: custom\nbase_branch: main\n---\n\nTask description";
    let (fm, body) = parse_task_markdown(input);
    let fm = fm.expect("should have frontmatter");
    assert_eq!(fm.agent.as_deref(), Some("custom"));
    assert_eq!(fm.base_branch.as_deref(), Some("main"));
    assert!(body.contains("Task description"));
  }

  #[test]
  fn parse_task_markdown_malformed_yaml_returns_none() {
    let input = "---\ninvalid: yaml: structure:\n---\n\nContent";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert!(body.contains("Content"));
  }

  #[test]
  fn parse_task_markdown_no_closing_delimiter() {
    let input = "---\nagent: test\nContent without closing";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert_eq!(body, input);
  }

  #[test]
  fn parse_task_markdown_preserves_body_whitespace() {
    let input = "---\nagent: test\n---\n\n\n  Indented content\n\n";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_some());
    assert!(body.starts_with("\n"));
    assert!(body.contains("  Indented content"));
  }

  #[test]
  fn parse_task_markdown_handles_dashes_in_body() {
    let input = "---\nagent: test\n---\n\n---\nThis is not frontmatter\n---\n";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_some());
    assert!(body.contains("---\nThis is not frontmatter\n---"));
  }

  #[test]
  fn read_task_frontmatter_returns_none_for_missing_file() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 999,
      slug: "nonexistent".to_string(),
    };
    let result = read_task_frontmatter(&paths, &task);
    assert!(result.is_none());
  }

  #[test]
  fn read_task_frontmatter_returns_some_when_present() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let tasks_dir = paths.tasks_dir();
    fs::create_dir_all(&tasks_dir).unwrap();

    let task = TaskRef {
      id: 1,
      slug: "test".to_string(),
    };
    let task_path = tasks_dir.join("1-test.md");
    fs::write(&task_path, "---\nagent: myagent\n---\n\nBody").unwrap();

    let result = read_task_frontmatter(&paths, &task);
    assert!(result.is_some());
    let fm = result.unwrap();
    assert_eq!(fm.agent.as_deref(), Some("myagent"));
  }

  #[test]
  fn read_task_frontmatter_returns_none_when_no_frontmatter() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let tasks_dir = paths.tasks_dir();
    fs::create_dir_all(&tasks_dir).unwrap();

    let task = TaskRef {
      id: 2,
      slug: "plain".to_string(),
    };
    let task_path = tasks_dir.join("2-plain.md");
    fs::write(&task_path, "Just plain content").unwrap();

    let result = read_task_frontmatter(&paths, &task);
    assert!(result.is_none());
  }

  #[test]
  fn write_task_content_adds_trailing_newline() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 1,
      slug: "test".to_string(),
    };

    let content = TaskContent {
      frontmatter: None,
      body: "No trailing newline".to_string(),
    };
    write_task_content(&paths, &task, &content).expect("write should succeed");

    let task_path = task_file(&paths, &task);
    let written = fs::read_to_string(task_path).expect("read file");
    assert!(written.ends_with('\n'));
  }

  #[test]
  fn write_task_content_preserves_existing_trailing_newline() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 1,
      slug: "test".to_string(),
    };

    let content = TaskContent {
      frontmatter: None,
      body: "Already has newline\n".to_string(),
    };
    write_task_content(&paths, &task, &content).expect("write should succeed");

    let task_path = task_file(&paths, &task);
    let written = fs::read_to_string(task_path).expect("read file");
    assert_eq!(written, "Already has newline\n");
    assert!(!written.ends_with("\n\n"));
  }

  #[test]
  fn write_and_read_task_content_roundtrip() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 3,
      slug: "roundtrip".to_string(),
    };

    let original = TaskContent {
      frontmatter: Some(TaskFrontmatter {
        agent: Some("test-agent".to_string()),
        base_branch: Some("feature".to_string()),
      }),
      body: "Test body content\nwith multiple lines\n".to_string(),
    };

    write_task_content(&paths, &task, &original).expect("write");
    let read_back = read_task_content(&paths, &task).expect("read");

    assert_eq!(read_back.frontmatter, original.frontmatter);
    assert_eq!(read_back.body, original.body);
  }
}
