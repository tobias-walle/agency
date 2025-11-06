use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
pub mod config;
pub mod pty;
mod utils;

use crate::config::load_config;
use crate::config::{AgencyPaths, AppContext};

/// Agency - An AI agent manager and orchestrator in your command line.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
  #[command(subcommand)]
  command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
  /// Create a new task under .agency/tasks
  New {
    slug: String,
    #[arg(long)]
    attach: bool,
  },
  /// Print the absolute worktree path
  Path { ident: String },
  /// Print the branch name
  Branch { ident: String },
  /// Remove task file, worktree, and branch
  Rm { ident: String },
  /// List tasks (ID and SLUG)
  Ps {},
  /// Daemon control commands
  Daemon {
    #[command(subcommand)]
    cmd: DaemonCmd,
  },
  /// Attach to task via PTY daemon
  Attach { task: String },
  /// Stop the active task session via daemon
  Stop { task: String },
}

#[derive(Debug, Subcommand)]
enum DaemonCmd {
  /// Start the daemon as a background service
  Start {},
  /// Stop the daemon gracefully
  Stop {},
  /// Restart the daemon
  Restart {},
  /// Run the daemon in the foreground (internal)
  #[command(hide = true)]
  Run {},
}

pub fn parse() -> Cli {
  Cli::parse()
}

pub fn run() -> Result<()> {
  let cli = parse();
  let cwd = std::env::current_dir()?;
  let paths = AgencyPaths::new(cwd.clone());
  let config = load_config(&cwd)?;
  let ctx = AppContext { paths, config };

  match cli.command {
    Some(Commands::New { slug, attach }) => {
      commands::new::run(&ctx, &slug)?;
      if attach {
        commands::daemon::start()?;
        commands::attach::run_with_task(&ctx, &slug)?;
      }
    }
    Some(Commands::Path { ident }) => {
      commands::path::run(&ctx, &ident)?;
    }
    Some(Commands::Branch { ident }) => {
      commands::branch::run(&ctx, &ident)?;
    }
    Some(Commands::Rm { ident }) => {
      commands::rm::run(&ctx, &ident)?;
    }
    Some(Commands::Ps {}) => {
      commands::ps::run(&ctx)?;
    }
    Some(Commands::Daemon { cmd }) => match cmd {
      DaemonCmd::Start {} => commands::daemon::start()?,
      DaemonCmd::Stop {} => commands::daemon::stop()?,
      DaemonCmd::Restart {} => commands::daemon::restart()?,
      DaemonCmd::Run {} => commands::daemon::run_blocking()?,
    },
    Some(Commands::Attach { task }) => {
      commands::attach::run_with_task(&ctx, &task)?;
    }
    Some(Commands::Stop { task: _task }) => {
      // Task validation can be added; currently stop daemon globally
      commands::daemon::stop()?;
    }
    None => {}
  }

  Ok(())
}
