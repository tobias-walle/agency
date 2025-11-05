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
  New { slug: String },
  /// Print the absolute worktree path
  Path { ident: String },
  /// Print the branch name
  Branch { ident: String },
  /// Remove task file, worktree, and branch
  Rm { ident: String },
  /// List tasks (ID and SLUG)
  Ps {},
  /// Start PTY daemon
  Daemon {},
  /// Attach to PTY daemon
  Attach {},
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
    Some(Commands::New { slug }) => {
      commands::new::run(&ctx, &slug)?;
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
    Some(Commands::Daemon {}) => {
      commands::daemon::run()?;
    }
    Some(Commands::Attach {}) => {
      commands::attach::run()?;
    }
    None => {}
  }

  Ok(())
}
