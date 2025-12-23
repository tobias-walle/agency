use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::{AgencyConfig, AgencyPaths, AppContext};
use crate::daemon_protocol::TaskMeta;
use crate::utils::editor::open_path as open_editor;
use crate::utils::git::head_branch;

static TASK_FILE_RE: OnceLock<Regex> = OnceLock::new();
static TRAILING_NUM_RE: OnceLock<Regex> = OnceLock::new();

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct TaskRef {
  pub id: u32,
  pub slug: String,
}

impl From<TaskMeta> for TaskRef {
  fn from(m: TaskMeta) -> Self {
    Self {
      id: m.id,
      slug: m.slug,
    }
  }
}

impl TaskRef {
  pub fn from_task_file(path: &Path) -> Option<Self> {
    let name = path.file_name()?.to_str()?;
    let re = TASK_FILE_RE.get_or_init(|| Regex::new(r"^(\d+)-(.+)\.md$").expect("valid regex"));
    let caps = re.captures(name)?;
    let id_str = caps.get(1)?.as_str();
    let slug = caps.get(2)?.as_str().to_string();
    let Ok(id) = id_str.parse::<u32>() else {
      return None;
    };
    Some(TaskRef { id, slug })
  }
}

/// Compute a unique slug by scanning the tasks dir and appending or incrementing a trailing number.
/// Examples:
/// - base `alpha` with existing {`alpha`} -> `alpha2`
/// - base `alpha2` with existing {`alpha2`} -> `alpha3`
/// - base `alpha` with existing {`alpha`, `alpha2`, `alpha3`} -> `alpha4`
pub fn compute_unique_slug(tasks: &Path, base: &str) -> Result<String> {
  // Collect existing slugs
  let mut existing: HashSet<String> = HashSet::new();
  if tasks.exists() {
    for entry in
      std::fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))?
    {
      let entry = entry?;
      let path = entry.path();
      if let Some(tf) = TaskRef::from_task_file(&path) {
        existing.insert(tf.slug);
      }
    }
  }

  // If the base isn't taken, use it directly
  if !existing.contains(base) {
    return Ok(base.to_string());
  }

  // Determine prefix and base numeric suffix (if any) using regex
  let re_trailing = TRAILING_NUM_RE
    .get_or_init(|| Regex::new(r"^(?P<prefix>.*?)(?P<num>\d+)$").expect("valid regex"));
  let (prefix, base_n): (&str, u64) = if let Some(c) = re_trailing.captures(base) {
    let p = c.name("prefix").map_or("", |m| m.as_str());
    let n = c
      .name("num")
      .map_or(0, |m| m.as_str().parse::<u64>().unwrap_or(0));
    (p, n)
  } else {
    (base, 0)
  };

  // Find the highest numeric suffix among existing slugs for this prefix
  let mut max_n = if base_n == 0 { 1 } else { base_n };
  let dyn_re =
    Regex::new(&format!(r"^{}(?P<num>\d+)?$", regex::escape(prefix))).expect("valid regex");
  for s in &existing {
    if let Some(caps) = dyn_re.captures(s) {
      let n = caps
        .name("num")
        .and_then(|m| m.as_str().parse::<u64>().ok())
        .unwrap_or(1);
      max_n = max_n.max(n);
    }
  }

  // Next available number
  let next = max_n.saturating_add(1);
  Ok(format!("{prefix}{next}"))
}

/// Compute the next global id as `max(existing_ids) + 1`.
pub fn next_id(tasks: &Path) -> Result<u32> {
  let mut max_id: u32 = 0;
  if tasks.exists() {
    for entry in
      std::fs::read_dir(tasks).with_context(|| format!("failed to read {}", tasks.display()))?
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

pub fn normalize_and_validate_slug(input: &str) -> Result<String> {
  // Slugify: lowercase, replace any non-alphanumeric with '-', collapse runs,
  // and trim leading/trailing '-'. Allow Unicode alphanumerics.
  let lowered = input.to_lowercase();
  let mut out = String::with_capacity(lowered.len());
  for ch in lowered.chars() {
    if ch.is_alphanumeric() {
      out.push(ch);
    } else if !out.ends_with('-') {
      out.push('-');
    }
  }
  // Trim leading/trailing '-'
  let trimmed = out.trim_matches('-').to_string();
  if trimmed.is_empty() {
    bail!("invalid slug: empty after slugify");
  }
  // Enforce starting with a letter to keep branch/task names readable
  if !trimmed
    .chars()
    .next()
    .is_some_and(|c| c.is_ascii_alphabetic())
  {
    bail!("invalid slug: must start with a letter");
  }
  Ok(trimmed)
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
  }
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskFrontmatter {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default)]
  pub base_branch: Option<String>,
}

/// Extension trait for `Option<TaskFrontmatter>` to extract base branch with fallback.
pub trait TaskFrontmatterExt {
  /// Returns the stored `base_branch` or falls back to the current HEAD branch.
  fn base_branch(&self, ctx: &AppContext) -> String;

  /// Returns the stored `base_branch` or computes a fallback using the provided function.
  /// Use this when `AppContext` is not available (e.g., in the daemon).
  fn base_branch_or<F>(&self, fallback: F) -> String
  where
    F: FnOnce() -> String;
}

impl TaskFrontmatterExt for Option<TaskFrontmatter> {
  fn base_branch(&self, ctx: &AppContext) -> String {
    self.base_branch_or(|| head_branch(ctx))
  }

  fn base_branch_or<F>(&self, fallback: F) -> String
  where
    F: FnOnce() -> String,
  {
    self
      .as_ref()
      .and_then(|f| f.base_branch.clone())
      .unwrap_or_else(fallback)
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskContent {
  pub frontmatter: Option<TaskFrontmatter>,
  pub body: String,
}

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

/// Resolve the effective agent for a task: front matter `agent` first,
/// then config default. Returns `None` if neither is set.
pub fn agent_for_task(cfg: &AgencyConfig, fm: Option<&TaskFrontmatter>) -> Option<String> {
  if let Some(fm) = fm
    && let Some(a) = &fm.agent
  {
    return Some(a.clone());
  }
  cfg.agent.clone()
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
    // Success cases with slugify behavior
    assert_eq!(
      normalize_and_validate_slug("Alpha World").unwrap(),
      "alpha-world"
    );
    assert_eq!(
      normalize_and_validate_slug("alpha_world").unwrap(),
      "alpha-world"
    );
    assert_eq!(
      normalize_and_validate_slug("alpha---world").unwrap(),
      "alpha-world"
    );
    // Starting with a digit should be rejected
    assert!(normalize_and_validate_slug("1invalid").is_err());

    // Error cases: empty or becomes empty after slugify
    assert!(normalize_and_validate_slug("").is_err());
    assert!(normalize_and_validate_slug("---").is_err());
    assert!(normalize_and_validate_slug("   ").is_err());
    assert!(normalize_and_validate_slug("**").is_err());
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
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let wt_dir = worktree_dir(&paths, &task);
    assert!(wt_dir.ends_with(".agency/worktrees/7-alpha"));

    let tf_path = task_file(&paths, &task);
    assert!(tf_path.ends_with(".agency/tasks/7-alpha.md"));
  }

  #[test]
  fn parse_task_markdown_with_agent() {
    let input = "---\nagent: sh\n---\n\n# Task 1: alpha\n";
    let (fm, body) = parse_task_markdown(input);
    let fm = fm.expect("front matter present");
    assert_eq!(fm.agent.as_deref(), Some("sh"));
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
    let input = "---\nagent: sh\n# Task 3: gamma\n"; // no closing delimiter
    let (fm, body) = parse_task_markdown(input);
    assert!(fm.is_none());
    assert_eq!(body, input);
  }

  #[test]
  fn resolve_id_or_slug_by_id_and_slug() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
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

  #[test]
  fn write_and_read_task_content_without_header() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 1,
      slug: "sample-task".to_string(),
    };
    let frontmatter = TaskFrontmatter {
      agent: Some("agent-one".to_string()),
      base_branch: Some("main".to_string()),
    };
    let body = "Implement the feature\nwith bullet points\n".to_string();
    let content = TaskContent {
      frontmatter: Some(frontmatter.clone()),
      body: body.clone(),
    };

    write_task_content(&paths, &task, &content).expect("write succeeds");
    let stored_path = task_file(&paths, &task);
    let stored = std::fs::read_to_string(stored_path).expect("read stored file");

    assert!(
      !stored.contains("# Sample Task"),
      "stored content should not contain generated headers"
    );

    let roundtrip = read_task_content(&paths, &task).expect("roundtrip read");
    assert_eq!(roundtrip.body, body);
    assert_eq!(roundtrip.frontmatter, Some(frontmatter));
  }

  #[test]
  fn write_task_content_preserves_trailing_newline() {
    let dir = TempDir::new().expect("tmp");
    let paths = AgencyPaths::new(dir.path(), dir.path());
    let task = TaskRef {
      id: 2,
      slug: "another-task".to_string(),
    };

    let content = TaskContent {
      frontmatter: None,
      body: "Single line body".to_string(),
    };
    write_task_content(&paths, &task, &content).expect("write succeeds");

    let stored_path = task_file(&paths, &task);
    let stored = std::fs::read_to_string(stored_path).expect("read stored file");
    assert!(
      stored.ends_with('\n'),
      "stored body should end with newline, got: {stored:?}"
    );
  }

  #[test]
  fn base_branch_or_returns_stored_value() {
    let fm: Option<TaskFrontmatter> = Some(TaskFrontmatter {
      agent: None,
      base_branch: Some("feature-branch".to_string()),
    });
    let result = fm.base_branch_or(|| "fallback".to_string());
    assert_eq!(result, "feature-branch");
  }

  #[test]
  fn base_branch_or_uses_fallback_when_none() {
    let fm: Option<TaskFrontmatter> = Some(TaskFrontmatter {
      agent: None,
      base_branch: None,
    });
    let result = fm.base_branch_or(|| "fallback".to_string());
    assert_eq!(result, "fallback");
  }

  #[test]
  fn base_branch_or_uses_fallback_when_no_frontmatter() {
    let fm: Option<TaskFrontmatter> = None;
    let result = fm.base_branch_or(|| "fallback".to_string());
    assert_eq!(result, "fallback");
  }
}
