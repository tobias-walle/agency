use super::types::{Config, ConfigError, Result};

pub(super) fn validate_agents(cfg: &Config) -> Result<()> {
  for (name, agent_cfg) in &cfg.agents {
    if agent_cfg.start.is_empty() {
      return Err(ConfigError::InvalidAgentDefinition {
        agent: name.to_string(),
      });
    }
  }

  if let Some(agent) = cfg.default_agent.as_ref() {
    let key = agent_key(agent).to_string();
    if !cfg.agents.contains_key(&key) {
      return Err(ConfigError::MissingAgentDefinition { agent: key });
    }
  }

  Ok(())
}

fn agent_key(agent: &crate::domain::task::Agent) -> &'static str {
  match agent {
    crate::domain::task::Agent::Opencode => "opencode",
    crate::domain::task::Agent::ClaudeCode => "claude-code",
    crate::domain::task::Agent::Fake => "fake",
  }
}
