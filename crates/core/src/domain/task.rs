use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TaskId(pub u64);

impl fmt::Display for TaskId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.0)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
  Draft,
  Running,
  Idle,
  Completed,
  Reviewed,
  Failed,
  Merged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
  Opencode,
  #[serde(rename = "claude-code")]
  ClaudeCode,
  Fake,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFrontMatter {
  pub base_branch: String,
  pub status: Status,
  #[serde(default)]
  pub labels: Vec<String>,
  pub created_at: DateTime<Utc>,
  pub agent: Agent,
  #[serde(default)]
  pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
  pub id: TaskId,
  pub slug: String,
  pub front_matter: TaskFrontMatter,
  pub body: String,
}

#[derive(Debug, Error)]
pub enum TaskError {
  #[error("invalid transition: {from:?} -> {to:?}")]
  InvalidTransition { from: Status, to: Status },
  #[error("invalid task filename: {0}")]
  InvalidFilename(String),
  #[error("missing front matter start delimiter '---'")]
  FrontMatterStartMissing,
  #[error("missing front matter end delimiter '---'")]
  FrontMatterEndMissing,
  #[error("yaml parse error: {0}")]
  YamlParse(#[from] serde_yaml::Error),
}

impl Task {
  pub fn can_transition(from: &Status, to: &Status) -> bool {
    use Status::*;
    matches!(
      (from, to),
      (Draft, Running)
        | (Running, Idle)
        | (Idle, Running)
        | (Running, Completed)
        | (Running, Failed)
        | (Running, Reviewed)
        | (Completed, Merged)
        | (Reviewed, Merged)
    )
  }

  pub fn transition_to(&mut self, new_status: Status) -> Result<(), TaskError> {
    let from = self.front_matter.status.clone();
    if Self::can_transition(&from, &new_status) {
      self.front_matter.status = new_status;
      Ok(())
    } else {
      Err(TaskError::InvalidTransition {
        from,
        to: new_status,
      })
    }
  }

  pub fn to_markdown(&self) -> Result<String, TaskError> {
    let yaml = serde_yaml::to_string(&self.front_matter)?;
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&yaml);
    // Ensure a closing fence and a blank line before body
    s.push_str("---\n\n");
    s.push_str(&self.body);
    if !self.body.ends_with('\n') {
      s.push('\n');
    }
    Ok(s)
  }

  pub fn from_markdown(id: TaskId, slug: String, content: &str) -> Result<Self, TaskError> {
    // Expect a front matter block starting at the beginning delimited by --- lines
    let content = content
      .strip_prefix("---\n")
      .ok_or(TaskError::FrontMatterStartMissing)?;
    let parts: Vec<&str> = content.splitn(2, "\n---\n").collect();
    if parts.len() != 2 {
      return Err(TaskError::FrontMatterEndMissing);
    }
    let fm_yaml = parts[0];
    let mut body = parts[1].to_string();
    if body.starts_with('\n') {
      body.remove(0);
    }
    let front_matter: TaskFrontMatter = serde_yaml::from_str(fm_yaml)?;
    Ok(Task {
      id,
      slug,
      front_matter,
      body,
    })
  }

  pub fn parse_filename(filename: &str) -> Result<(TaskId, String), TaskError> {
    // Accept path or bare filename; match last segment
    let name = filename.rsplit('/').next().unwrap_or(filename);
    let re: &Regex = get_file_regex();
    if let Some(caps) = re.captures(name) {
      let id: u64 = caps[1]
        .parse()
        .map_err(|_| TaskError::InvalidFilename(filename.to_string()))?;
      let slug = caps[2].to_string();
      Ok((TaskId(id), slug))
    } else {
      Err(TaskError::InvalidFilename(filename.to_string()))
    }
  }

  pub fn format_filename(id: TaskId, slug: &str) -> String {
    format!("{}-{}.md", id.0, slug)
  }
}

fn get_file_regex() -> &'static Regex {
  // ^(\d+)-([A-Za-z0-9-]+)\.md$
  static ONCE_CELL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
  ONCE_CELL.get_or_init(|| Regex::new(r"^(\d+)-([A-Za-z0-9-]+)\.md$").expect("valid regex"))
}

#[cfg(test)]
mod tests {
  use super::*;
  use proptest::prelude::*;

  fn sample_front_matter() -> TaskFrontMatter {
    TaskFrontMatter {
      base_branch: "main".to_string(),
      status: Status::Draft,
      labels: vec!["a".into(), "b".into()],
      created_at: Utc::now(),
      agent: Agent::Fake,
      session_id: None,
    }
  }

  #[test]
  fn yaml_round_trip() {
    let task = Task {
      id: TaskId(42),
      slug: "test-slug".into(),
      front_matter: sample_front_matter(),
      body: "Hello world".into(),
    };
    let md = task.to_markdown().expect("serialize");
    let parsed = Task::from_markdown(task.id, task.slug.clone(), &md).expect("parse");
    assert_eq!(
      parsed.front_matter.base_branch,
      task.front_matter.base_branch
    );
    assert_eq!(parsed.front_matter.status, task.front_matter.status);
    assert_eq!(parsed.front_matter.labels, task.front_matter.labels);
    assert_eq!(parsed.front_matter.agent, task.front_matter.agent);
    assert_eq!(parsed.front_matter.session_id, task.front_matter.session_id);
    assert_eq!(parsed.body, format!("{}\n", task.body));
  }

  #[test]
  fn transitions_enforced() {
    let mut task = Task {
      id: TaskId(1),
      slug: "s".into(),
      front_matter: sample_front_matter(),
      body: String::new(),
    };
    // draft -> running ok
    task.transition_to(Status::Running).expect("draft->running");
    // running -> completed ok
    task
      .transition_to(Status::Completed)
      .expect("running->completed");
    // completed -> merged ok
    task
      .transition_to(Status::Merged)
      .expect("completed->merged");
    // merged -> running not allowed
    let err = task.transition_to(Status::Running).unwrap_err();
    match err {
      TaskError::InvalidTransition { .. } => {}
      _ => panic!("wrong error"),
    }
  }

  #[test]
  fn filename_parse_and_format() {
    let (id, slug) = Task::parse_filename("123-hello-world.md").expect("parse");
    assert_eq!(id, TaskId(123));
    assert_eq!(slug, "hello-world");
    let name = Task::format_filename(id, &slug);
    assert_eq!(name, "123-hello-world.md");
  }

  proptest! {
    #[test]
    fn filename_parsing_is_inverse(id in 1u64.., slug in "[A-Za-z0-9-]{1,32}") {
      let name = format!("{}-{}.md", id, slug);
      let (pid, pslug) = Task::parse_filename(&name).unwrap();
      prop_assert_eq!(pid.0, id);
      prop_assert_eq!(pslug, slug);
    }
  }
}
