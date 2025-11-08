use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::AgencyPaths;

pub struct TaskRef {
  pub id: u32,
  pub slug: String,
}

impl TaskRef {
  pub fn from_task_file(path: &Path) -> Option<Self> {
    let name = path.file_name()?.to_str()?;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^(\d+)-(.+)\.md$").expect("valid regex"));
    let caps = re.captures(name)?;
    let id_str = caps.get(1)?.as_str();
    let slug = caps.get(2)?.as_str().to_string();
    let Ok(id) = id_str.parse::<u32>() else {
      return None;
    };
    Some(TaskRef { id, slug })
  }
}

pub fn normalize_and_validate_slug(input: &str) -> Result<String> {
  let lowered = input.to_lowercase();
  if lowered.is_empty() {
    bail!("invalid slug: empty");
  }
  let mut chars = lowered.chars();
  let Some(first) = chars.next() else {
    bail!("invalid slug: empty");
  };
  if !first.is_alphabetic() {
    bail!("invalid slug: must start with a letter");
  }
  if !chars.all(|c| c.is_alphanumeric() || c == '-') {
    bail!("invalid slug: only Unicode letters, digits and '-' allowed");
  }
  Ok(lowered)
}

pub fn resolve_id_or_slug(paths: &AgencyPaths, ident: &str) -> Result<TaskRef> {
  let tasks = paths.tasks_dir();
  if !tasks.exists() {
    bail!("tasks dir not found at {}", tasks.display());
  }
  if ident.chars().all(|c| c.is_ascii_digit()) {
    let id: u32 = ident.parse().context("invalid id")?;
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
    bail!("task with id {ident} not found");
  } else {
    let slug = ident.to_string();
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
    bail!("task with slug {ident} not found");
  }
}

pub fn branch_name(task: &TaskRef) -> String {
  format!("agency/{}-{}", task.id, task.slug)
}

pub fn worktree_name(task: &TaskRef) -> String {
  format!("{}-{}", task.id, task.slug)
}

pub fn worktree_dir(paths: &AgencyPaths, task: &TaskRef) -> PathBuf {
  paths.worktrees_dir().join(worktree_name(task))
}

pub fn task_file(paths: &AgencyPaths, task: &TaskRef) -> PathBuf {
  paths
    .tasks_dir()
    .join(format!("{}-{}.md", task.id, task.slug))
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskFrontmatter {
  #[serde(default)]
  pub agent: Option<String>,
}

const FRONT_MATTER_START: &str = "---\n";
const FRONT_MATTER_END: &str = "\n---\n";

/// Parse optional YAML front matter from the start of a task markdown string.
/// Returns the parsed front matter (if present) and a slice of the body
/// excluding the front matter. Gracefully ignores malformed boundaries.
pub fn parse_task_markdown(input: &str) -> (Option<TaskFrontmatter>, &str) {
  // Expect YAML front matter delimited by FRONT_MATTER_START and FRONT_MATTER_END.
  if let Some(rest) = input.strip_prefix(FRONT_MATTER_START) {
    if let Some((yaml, body)) = rest.split_once(FRONT_MATTER_END) {
      let fm = serde_yaml::from_str::<TaskFrontmatter>(yaml).ok();
      return (fm, body);
    }
  }
  (None, input)
}

/// Format task markdown with optional YAML front matter and a standard title line.
pub fn format_task_markdown(
  id: u32,
  slug: &str,
  frontmatter: Option<&TaskFrontmatter>,
) -> Result<String> {
  let title = format!("# Task {id}: {slug}\n");
  if let Some(fm) = frontmatter {
    let yaml = serde_yaml::to_string(fm).context("failed to serialize front matter")?;
    Ok(format!(
      "{start}{yaml}{end}\n{title}",
      start = FRONT_MATTER_START,
      yaml = yaml,
      end = FRONT_MATTER_END,
      title = title
    ))
  } else {
    Ok(title)
  }
}

#[cfg(test)]
mod tests {

  use super::*;
  use tempfile::TempDir;

  #[test]
  fn parses_valid_task_file() {
    let dir = TempDir::new().expect("tmp");
    let file = dir.path().join("12-sample-task.md");
    std::fs::write(&file, "# test\n").unwrap();
    let tf = TaskRef::from_task_file(&file).expect("should parse valid");
    assert_eq!(tf.id, 12);
    assert_eq!(tf.slug, "sample-task");
  }

  #[test]
  fn normalize_and_validate_slug_rules() {
    assert!(normalize_and_validate_slug("alpha-1").is_ok());
    assert!(normalize_and_validate_slug("").is_err());
    assert!(normalize_and_validate_slug("1invalid").is_err());
    assert!(normalize_and_validate_slug("bad/slug").is_err());
  }

  #[test]
  fn resolve_names_and_paths() {
    let task = TaskRef {
      id: 7,
      slug: "alpha".to_string(),
    };
    assert_eq!(branch_name(&task), "agency/7-alpha");
    assert_eq!(worktree_name(&task), "7-alpha");

    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path());
    let wt_dir = worktree_dir(&paths, &task);
    assert!(wt_dir.ends_with(".agency/worktrees/7-alpha"));

    let tf_path = task_file(&paths, &task);
    assert!(tf_path.ends_with(".agency/tasks/7-alpha.md"));
  }

  #[test]
  fn parse_task_markdown_with_agent() {
    let input = "---\nagent: fake\n---\n\n# Task 1: alpha\n";
    let (fm, body) = parse_task_markdown(input);
    let fm = fm.expect("front matter present");
    assert_eq!(fm.agent.as_deref(), Some("fake"));
    assert!(body.starts_with("\n# Task 1: alpha\n") || body.starts_with("# Task 1: alpha\n"));
  }

  #[test]
  fn parse_task_markdown_without_front_matter() {
    let input = "# Task 2: beta\n";
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert_eq!(body, input);
  }

  #[test]
  fn parse_task_markdown_ignores_unclosed_block() {
    let input = "---\nagent: fake\n# Task 3: gamma\n"; // no closing delimiter
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert_eq!(body, input);
  }

  #[test]
  fn resolve_id_or_slug_by_id_and_slug() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path());
    let tasks = paths.tasks_dir();
    std::fs::create_dir_all(&tasks).unwrap();

    std::fs::write(tasks.join("1-foo.md"), "# foo\n").unwrap();
    std::fs::write(tasks.join("2-bar.md"), "# bar\n").unwrap();

    let r1 = resolve_id_or_slug(&paths, "1").expect("id 1 present");
    assert_eq!(r1.id, 1);
    assert_eq!(r1.slug, "foo");

    let r2 = resolve_id_or_slug(&paths, "bar").expect("slug bar present");
    assert_eq!(r2.id, 2);
    assert_eq!(r2.slug, "bar");

    let not_found = resolve_id_or_slug(&paths, "baz");
    assert!(not_found.is_err());
  }
}
