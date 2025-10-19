use clap::{Args as ClapArgs, CommandFactory, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(version, about = "Agency CLI", long_about = None, bin_name = "agency")]
pub struct Cli {
  #[command(subcommand)]
  pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
  /// Daemon related commands
  Daemon(DaemonArgs),
  /// Create project scaffolding and config
  Init,
  /// Create a new task
  New(NewArgs),
  /// Start a task
  Start(StartArgs),
  /// Show task status list
  Status,
  /// Attach to a task's PTY session
  Attach(AttachArgs),
  /// Print the task worktree path
  Path(PathArgs),
  /// Print a shell hook function to cd into a task's worktree
  ShellHook,
}

#[derive(Debug, ClapArgs)]
pub struct NewArgs {
  /// Task slug (kebab-case)
  pub slug: String,
  /// Base branch to branch from
  #[arg(long, default_value = "main")]
  pub base_branch: String,
  /// Optional label (repeatable)
  #[arg(long = "label")]
  pub labels: Vec<String>,
  /// Agent to use (opencode|claude-code|fake). Optional if configured via default_agent.
  #[arg(long, value_enum)]
  pub agent: Option<AgentArg>,
  /// Create task without starting it
  #[arg(long)]
  pub draft: bool,
  /// Do not auto-attach after creation/start
  #[arg(long = "no-attach")]
  pub no_attach: bool,
  /// Message/description body to store; if omitted, opens $EDITOR
  #[arg(short = 'm', long = "message")]
  pub message: Option<String>,
}

#[derive(Debug, ClapArgs)]
pub struct PathArgs {
  /// Task reference: numeric id or slug
  pub task: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AgentArg {
  #[value(name = "opencode")]
  Opencode,
  #[value(name = "claude-code")]
  ClaudeCode,
  #[value(name = "fake")]
  Fake,
}

#[derive(Debug, ClapArgs)]
pub struct StartArgs {
  /// Task reference: numeric id or slug
  pub task: String,
}

#[derive(Debug, ClapArgs)]
pub struct AttachArgs {
  /// Task reference: numeric id or slug
  pub task: String,
  /// Attach without replaying prior PTY output
  #[arg(long = "no-replay")]
  pub no_replay: bool,
}

#[derive(Debug, ClapArgs)]
pub struct DaemonArgs {
  #[command(subcommand)]
  pub command: DaemonSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum DaemonSubcommand {
  /// Show daemon status
  Status,
  /// Start the daemon
  Start,
  /// Stop the daemon
  Stop,
  /// Run the daemon (foreground)
  Run,
  /// Restart the daemon
  Restart,
}

impl Cli {
  pub fn print_help_and_exit() {
    let mut cmd = Cli::command();
    cmd.print_help().expect("print help");
    println!();
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::{CommandFactory, Parser, error::ErrorKind};

  #[test]
  fn help_flag_triggers_displayhelp() {
    let err = Cli::try_parse_from(["agency", "--help"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayHelp);
  }

  #[test]
  fn version_flag_triggers_displayversion() {
    let err = Cli::try_parse_from(["agency", "--version"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
  }

  #[test]
  fn command_factory_builds() {
    let _ = Cli::command();
  }
}
