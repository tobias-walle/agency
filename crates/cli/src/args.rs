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
  /// Agent to use (opencode|claude-code|fake)
  #[arg(long, value_enum, default_value = "fake")]
  pub agent: AgentArg,
  /// Create task without starting it
  #[arg(long)]
  pub draft: bool,
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
