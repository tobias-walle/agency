use clap::{Args as ClapArgs, CommandFactory, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(author, version, about = "Orchestra CLI", long_about = None, bin_name = "orchestra")]
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
  /// Attach to a task's PTY session
  Attach(AttachArgs),
}

#[derive(Debug, ClapArgs)]
pub struct AttachArgs {
  /// Task reference: numeric id or slug
  pub task: String,
  /// Custom detach keys (e.g., "ctrl-p,ctrl-q" or "ctrl-q").
  #[arg(long = "detach-keys")]
  pub detach_keys: Option<String>,
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
}

impl Cli {
  pub fn print_help_and_exit() {
    let mut cmd = Cli::command();
    cmd.print_help().expect("print help");
    println!();
  }
}
