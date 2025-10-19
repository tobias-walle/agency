use std::collections::BTreeMap;

use super::types::AgentConfig;

pub(crate) fn builtin_agents() -> BTreeMap<String, AgentConfig> {
  let mut agents = BTreeMap::new();
  agents.insert(
    "opencode".to_string(),
    AgentConfig {
      display_name: Some("OpenCode".to_string()),
      start: vec![
        "opencode".to_string(),
        "--agent".to_string(),
        "plan".to_string(),
        "-p".to_string(),
        "$AGENCY_PROMPT".to_string(),
      ],
      resume: None,
      run: None,
    },
  );
  agents.insert(
    "fake".to_string(),
    AgentConfig {
      display_name: Some("Shell".to_string()),
      start: vec!["sh".to_string()],
      resume: None,
      run: None,
    },
  );
  agents
}
