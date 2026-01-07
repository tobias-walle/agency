use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::{AgencyConfig, AppContext};
use crate::daemon_protocol::TaskMeta;
use crate::utils::git::head_branch;

static TASK_FILE_RE: OnceLock<Regex> = OnceLock::new();

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
  use std::path::PathBuf;

  #[test]
  fn task_ref_from_task_file_parses_valid() {
    let path = PathBuf::from("/tmp/42-example-task.md");
    let task = TaskRef::from_task_file(&path).expect("should parse");
    assert_eq!(task.id, 42);
    assert_eq!(task.slug, "example-task");
  }

  #[test]
  fn task_ref_from_task_file_handles_single_digit_id() {
    let path = PathBuf::from("/tmp/1-a.md");
    let task = TaskRef::from_task_file(&path).expect("should parse");
    assert_eq!(task.id, 1);
    assert_eq!(task.slug, "a");
  }

  #[test]
  fn task_ref_from_task_file_handles_large_id() {
    let path = PathBuf::from("/tmp/999999-big-task.md");
    let task = TaskRef::from_task_file(&path).expect("should parse");
    assert_eq!(task.id, 999999);
    assert_eq!(task.slug, "big-task");
  }

  #[test]
  fn task_ref_from_task_file_rejects_missing_extension() {
    let path = PathBuf::from("/tmp/5-task");
    assert!(TaskRef::from_task_file(&path).is_none());
  }

  #[test]
  fn task_ref_from_task_file_rejects_wrong_extension() {
    let path = PathBuf::from("/tmp/5-task.txt");
    assert!(TaskRef::from_task_file(&path).is_none());
  }

  #[test]
  fn task_ref_from_task_file_rejects_missing_slug() {
    let path = PathBuf::from("/tmp/5-.md");
    assert!(TaskRef::from_task_file(&path).is_none());
  }

  #[test]
  fn task_ref_from_task_file_rejects_missing_id() {
    let path = PathBuf::from("/tmp/-task.md");
    assert!(TaskRef::from_task_file(&path).is_none());
  }

  #[test]
  fn task_ref_from_task_file_rejects_no_hyphen() {
    let path = PathBuf::from("/tmp/5task.md");
    assert!(TaskRef::from_task_file(&path).is_none());
  }

  #[test]
  fn task_ref_from_task_file_rejects_invalid_id() {
    let path = PathBuf::from("/tmp/abc-task.md");
    assert!(TaskRef::from_task_file(&path).is_none());
  }

  #[test]
  fn task_ref_from_task_file_handles_slug_with_numbers() {
    let path = PathBuf::from("/tmp/10-task-v2-final.md");
    let task = TaskRef::from_task_file(&path).expect("should parse");
    assert_eq!(task.id, 10);
    assert_eq!(task.slug, "task-v2-final");
  }

  #[test]
  fn agent_for_task_prefers_frontmatter_agent() {
    let cfg = AgencyConfig {
      agent: Some("default-agent".to_string()),
      ..Default::default()
    };
    let fm = TaskFrontmatter {
      agent: Some("custom-agent".to_string()),
      base_branch: None,
    };
    let result = agent_for_task(&cfg, Some(&fm));
    assert_eq!(result, Some("custom-agent".to_string()));
  }

  #[test]
  fn agent_for_task_falls_back_to_config_default() {
    let cfg = AgencyConfig {
      agent: Some("default-agent".to_string()),
      ..Default::default()
    };
    let fm = TaskFrontmatter {
      agent: None,
      base_branch: None,
    };
    let result = agent_for_task(&cfg, Some(&fm));
    assert_eq!(result, Some("default-agent".to_string()));
  }

  #[test]
  fn agent_for_task_returns_none_when_both_missing() {
    let cfg = AgencyConfig {
      agent: None,
      ..Default::default()
    };
    let fm = TaskFrontmatter {
      agent: None,
      base_branch: None,
    };
    let result = agent_for_task(&cfg, Some(&fm));
    assert_eq!(result, None);
  }

  #[test]
  fn agent_for_task_uses_config_when_no_frontmatter() {
    let cfg = AgencyConfig {
      agent: Some("default-agent".to_string()),
      ..Default::default()
    };
    let result = agent_for_task(&cfg, None);
    assert_eq!(result, Some("default-agent".to_string()));
  }

  #[test]
  fn agent_for_task_returns_none_when_no_frontmatter_and_no_config() {
    let cfg = AgencyConfig {
      agent: None,
      ..Default::default()
    };
    let result = agent_for_task(&cfg, None);
    assert_eq!(result, None);
  }
}
