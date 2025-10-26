use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod config;
mod utils;

use crate::config::AgencyConfig;

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
}

pub fn parse() -> Cli {
  Cli::parse()
}

pub fn run() -> Result<()> {
  let cli = parse();
  let cwd = std::env::current_dir()?;
  let cfg = AgencyConfig::new(cwd);

  match cli.command {
    Some(Commands::New { slug }) => {
      commands::new::run(&cfg, &slug)?;
    }
    Some(Commands::Path { ident }) => {
      commands::path::run(&cfg, &ident)?;
    }
    Some(Commands::Branch { ident }) => {
      commands::branch::run(&cfg, &ident)?;
    }
    Some(Commands::Rm { ident }) => {
      commands::rm::run(&cfg, &ident)?;
    }
    Some(Commands::Ps {}) => {
      commands::ps::run(&cfg)?;
    }
    None => {}
  }

  Ok(())
}
