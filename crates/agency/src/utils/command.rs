use anyhow::{Result, bail};
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
    let args = if argv.len() > 1 {
      argv[1..].to_vec()
    } else {
      Vec::new()
    };
    let cwd = std::env::current_dir()?;
    Ok(Self {
      program,
      args,
      cwd,
      env: Vec::new(),
    })
  }
}

/// Expand "$VAR" references in argv using the given env map. Does not support ${} forms.
pub fn expand_vars_in_argv(argv: &[String], env: &HashMap<String, String>) -> Vec<String> {
  argv
    .iter()
    .map(|s| {
      let mut out = String::new();
      let bytes = s.as_bytes();
      let mut i = 0;
      while i < bytes.len() {
        if bytes[i] == b'$' {
          // capture variable name [A-Za-z_][A-Za-z0-9_]*
          let mut j = i + 1;
          while j < bytes.len() {
            let c = bytes[j] as char;
            if j == i + 1 {
              if !(c.is_ascii_alphabetic() || c == '_') {
                break;
              }
            } else if !(c.is_ascii_alphanumeric() || c == '_') {
              break;
            }
            j += 1;
          }
          if j > i + 1 {
            let key = &s[i + 1..j];
            if let Some(val) = env.get(key) {
              out.push_str(val);
            }
            i = j;
            continue;
          }
        }
        out.push(bytes[i] as char);
        i += 1;
      }
      out
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::Command;
  use crate::config::AgentConfig;
  use anyhow::Result;

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
}
