use anyhow::{Result, bail};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
  pub program: String,
  pub args: Vec<String>,
  pub cwd: PathBuf,
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
    Ok(Self { program, args, cwd })
  }
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
      "error message should mention empty: {}",
      msg
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
