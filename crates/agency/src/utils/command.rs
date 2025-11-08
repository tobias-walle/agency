use anyhow::{Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
  pub program: String,
  pub args: Vec<String>,
  pub cwd: PathBuf,
  pub env: Vec<(String, String)>,
}

impl Command {
  /// Construct from argv-like vector: first element is the program, rest are args.
  pub fn new(argv: &[String]) -> Result<Self> {
    if argv.is_empty() {
      bail!("command is empty");
    }
    let program = argv[0].clone();
    if program.trim().is_empty() {
      bail!("command program is empty");
    }
    let tail_args = if argv.len() > 1 {
      argv[1..].to_vec()
    } else {
      Vec::new()
    };
    let cwd = std::env::current_dir()?;
    Ok(Self {
      program,
      args: tail_args,
      cwd,
      env: Vec::new(),
    })
  }
}

/// Expand "$VAR" references in argv using the given env map. Does not support ${} forms.
pub fn expand_vars_in_argv(argv: &[String], env: &HashMap<String, String>) -> Vec<String> {
  let re = Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("valid var regex");
  argv
    .iter()
    .map(|input| {
      re.replace_all(input, |caps: &regex::Captures| {
        env.get(&caps[1]).map_or("", String::as_str)
      })
      .to_string()
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::Command;
  use super::expand_vars_in_argv;
  use crate::config::AgentConfig;
  use anyhow::Result;
  use std::collections::HashMap;

  #[test]
  fn command_new_errors_on_empty() {
    let argv: Vec<String> = vec![];
    let err = Command::new(&argv).expect_err("should error on empty argv");
    let msg = err.to_string();
    assert!(
      msg.contains("empty"),
      "error message should mention empty: {msg}"
    );
  }

  #[test]
  fn command_new_parses_single_program() -> Result<()> {
    let argv = vec!["sh".to_string()];
    let cmd = Command::new(&argv)?;
    assert_eq!(cmd.program, "sh");
    assert!(cmd.args.is_empty());
    Ok(())
  }

  #[test]
  fn command_new_parses_with_args() -> Result<()> {
    let argv = vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()];
    let cmd = Command::new(&argv)?;
    assert_eq!(cmd.program, "sh");
    assert_eq!(cmd.args, vec!["-c", "echo hi"]);
    Ok(())
  }

  #[test]
  fn agent_config_get_cmd_success() -> Result<()> {
    let ac = AgentConfig {
      cmd: vec!["echo".to_string(), "hello".to_string()],
    };
    let cmd = ac.get_cmd("echo-agent")?;
    assert_eq!(cmd.program, "echo");
    assert_eq!(cmd.args, vec!["hello"]);
    Ok(())
  }

  #[test]
  fn agent_config_get_cmd_errors_on_empty() {
    let ac = AgentConfig { cmd: vec![] };
    let err = ac.get_cmd("x").expect_err("should fail");
    let msg = err.to_string();
    assert!(msg.contains("not defined"));
  }

  #[test]
  fn expand_vars_in_argv_supports_sh_dash_c_with_task() {
    let mut env = HashMap::new();
    env.insert("AGENCY_TASK".to_string(), "Do something".to_string());
    let argv = vec![
      "sh".to_string(),
      "-c".to_string(),
      "echo Task: $AGENCY_TASK".to_string(),
    ];
    let expanded = expand_vars_in_argv(&argv, &env);
    assert_eq!(expanded[0], "sh");
    assert_eq!(expanded[1], "-c");
    assert_eq!(expanded[2], "echo Task: Do something");
  }

  #[test]
  fn expand_vars_in_argv_unknown_var_becomes_empty_string() {
    let env = HashMap::new();
    let argv = vec!["echo".to_string(), "$UNKNOWN".to_string()];
    let expanded = expand_vars_in_argv(&argv, &env);
    assert_eq!(expanded, vec!["echo", ""]);
  }

  #[test]
  fn expand_vars_in_argv_mixed_text_expands_inline() {
    let mut env = HashMap::new();
    env.insert("AGENCY_TASK".to_string(), "X".to_string());
    let argv = vec!["echo".to_string(), "pre-$AGENCY_TASK-post".to_string()];
    let expanded = expand_vars_in_argv(&argv, &env);
    assert_eq!(expanded, vec!["echo", "pre-X-post"]);
  }
}
