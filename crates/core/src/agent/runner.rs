use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;

use crate::config::{AgentConfig, Config};
use crate::domain::task::{Agent, TaskId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentAction {
  Start,
  Resume,
  Run,
}

impl AgentAction {
  fn as_str(&self) -> &'static str {
    match self {
      AgentAction::Start => "start",
      AgentAction::Resume => "resume",
      AgentAction::Run => "run",
    }
  }
}

#[derive(Debug, Error)]
pub enum AgentRunnerError {
  #[error("agent `{0}` is not configured")]
  MissingAgent(String),
  #[error("agent `{agent}` action `{action}` is not configured")]
  MissingAction { agent: String, action: &'static str },
  #[error("agent `{agent}` action `{action}` command must not be empty")]
  EmptyAction { agent: String, action: &'static str },
}

pub type RunnerResult<T> = Result<T, AgentRunnerError>;

pub fn build_env(
  task_id: TaskId,
  slug: &str,
  body: &str,
  prompt: &str,
  project_root: &Path,
  worktree_path: &Path,
  session_id: Option<&str>,
  message: Option<&str>,
) -> HashMap<String, String> {
  let mut env = HashMap::new();
  env.insert("AGENCY_TASK_ID".to_string(), task_id.0.to_string());
  env.insert("AGENCY_SLUG".to_string(), slug.to_string());
  env.insert("AGENCY_BODY".to_string(), body.to_string());
  env.insert("AGENCY_PROMPT".to_string(), prompt.to_string());
  env.insert(
    "AGENCY_PROJECT_ROOT".to_string(),
    path_to_string(project_root),
  );
  env.insert("AGENCY_WORKTREE".to_string(), path_to_string(worktree_path));
  if let Some(value) = session_id {
    env.insert("AGENCY_SESSION_ID".to_string(), value.to_string());
  }
  if let Some(value) = message {
    env.insert("AGENCY_MESSAGE".to_string(), value.to_string());
  }
  env
}

pub fn substitute_tokens(args: &[String], env: &HashMap<String, String>) -> Vec<String> {
  args
    .iter()
    .map(|arg| {
      let mut substituted = arg.clone();
      for (env_key, env_value) in env {
        let token = format!("${}", env_key);
        if substituted.contains(&token) {
          substituted = substituted.replace(&token, env_value);
        }
      }
      substituted
    })
    .collect()
}

pub fn resolve_action(
  config: &Config,
  agent: &Agent,
  action: AgentAction,
) -> RunnerResult<(String, Vec<String>)> {
  let key = agent_key(agent);
  let agent_cfg = config
    .agents
    .get(key)
    .ok_or_else(|| AgentRunnerError::MissingAgent(key.to_string()))?;
  let command = action_command(agent_cfg, key, action)?;
  if command.is_empty() {
    return Err(AgentRunnerError::EmptyAction {
      agent: key.to_string(),
      action: action.as_str(),
    });
  }
  let program = command[0].clone();
  let args = command.iter().skip(1).cloned().collect();
  Ok((program, args))
}

fn action_command<'a>(
  agent_cfg: &'a AgentConfig,
  agent_key: &str,
  action: AgentAction,
) -> RunnerResult<&'a [String]> {
  let command = match action {
    AgentAction::Start => Some(agent_cfg.start.as_slice()),
    AgentAction::Resume => agent_cfg.resume.as_deref(),
    AgentAction::Run => agent_cfg.run.as_deref(),
  };
  command.ok_or_else(|| AgentRunnerError::MissingAction {
    agent: agent_key.to_string(),
    action: action.as_str(),
  })
}

fn agent_key(agent: &Agent) -> &'static str {
  match agent {
    Agent::Opencode => "opencode",
    Agent::ClaudeCode => "claude-code",
    Agent::Fake => "fake",
  }
}

fn path_to_string(path: &Path) -> String {
  path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::Path;

  #[test]
  fn build_env_populates_expected_keys() {
    let env = build_env(
      TaskId(42),
      "hello-world",
      "body text",
      "prompt text",
      Path::new("/tmp/project"),
      Path::new("/tmp/project/.agency/worktrees/42-hello-world"),
      Some("session-123"),
      Some("message-abc"),
    );
    assert_eq!(env.get("AGENCY_TASK_ID").map(String::as_str), Some("42"));
    assert_eq!(env.get("AGENCY_SLUG").map(String::as_str), Some("hello-world"));
    assert_eq!(env.get("AGENCY_BODY").map(String::as_str), Some("body text"));
    assert_eq!(env.get("AGENCY_PROMPT").map(String::as_str), Some("prompt text"));
    assert_eq!(
      env.get("AGENCY_PROJECT_ROOT").map(String::as_str),
      Some("/tmp/project"),
    );
    assert_eq!(
      env.get("AGENCY_WORKTREE").map(String::as_str),
      Some("/tmp/project/.agency/worktrees/42-hello-world"),
    );
    assert_eq!(
      env.get("AGENCY_SESSION_ID").map(String::as_str),
      Some("session-123"),
    );
    assert_eq!(
      env.get("AGENCY_MESSAGE").map(String::as_str),
      Some("message-abc"),
    );
  }

  #[test]
  fn build_env_omits_optional_when_none() {
    let env = build_env(
      TaskId(7),
      "slug",
      "body",
      "prompt",
      Path::new("/project"),
      Path::new("/project/.agency/worktrees/7-slug"),
      None,
      None,
    );
    assert!(!env.contains_key("AGENCY_SESSION_ID"));
    assert!(!env.contains_key("AGENCY_MESSAGE"));
  }

  #[test]
  fn substitute_tokens_replaces_matching_placeholders() {
    let mut env = HashMap::new();
    env.insert("AGENCY_PROMPT".to_string(), "content".to_string());
    env.insert("AGENCY_SLUG".to_string(), "slug-value".to_string());
    let args = vec![
      "opencode".to_string(),
      "$AGENCY_PROMPT".to_string(),
      "slug:$AGENCY_SLUG".to_string(),
      "$UNKNOWN".to_string(),
    ];
    let substituted = substitute_tokens(&args, &env);
    assert_eq!(
      substituted,
      vec![
        "opencode".to_string(),
        "content".to_string(),
        "slug:slug-value".to_string(),
        "$UNKNOWN".to_string(),
      ],
    );
  }

  #[test]
  fn resolve_action_returns_program_and_args() {
    let config = Config::default();
    let (program, args) = resolve_action(&config, &Agent::Opencode, AgentAction::Start).unwrap();
    assert_eq!(program, "opencode");
    assert_eq!(
      args,
      vec![
        "--agent".to_string(),
        "plan".to_string(),
        "-p".to_string(),
        "$AGENCY_PROMPT".to_string(),
      ],
    );
  }

  #[test]
  fn resolve_action_missing_agent_errors() {
    let config = Config::default();
    let err = resolve_action(&config, &Agent::ClaudeCode, AgentAction::Start).unwrap_err();
    match err {
      AgentRunnerError::MissingAgent(name) => assert_eq!(name, "claude-code"),
      _ => panic!("unexpected error"),
    }
  }

  #[test]
  fn resolve_action_missing_action_errors() {
    let config = Config::default();
    let err = resolve_action(&config, &Agent::Fake, AgentAction::Run).unwrap_err();
    match err {
      AgentRunnerError::MissingAction { agent, action } => {
        assert_eq!(agent, "fake".to_string());
        assert_eq!(action, "run");
      }
      _ => panic!("unexpected error"),
    }
  }

  #[test]
  fn resolve_action_empty_command_errors() {
    let mut config = Config::default();
    config.agents.insert(
      "claude-code".to_string(),
      AgentConfig {
        display_name: Some("Claude".to_string()),
        start: vec!["claude".to_string()],
        resume: None,
        run: Some(Vec::new()),
      },
    );
    let err = resolve_action(&config, &Agent::ClaudeCode, AgentAction::Run).unwrap_err();
    match err {
      AgentRunnerError::EmptyAction { agent, action } => {
        assert_eq!(agent, "claude-code".to_string());
        assert_eq!(action, "run");
      }
      _ => panic!("unexpected error"),
    }
  }
}
