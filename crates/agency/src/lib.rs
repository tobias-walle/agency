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
    /// Select agent to attach to task (writes YAML front matter)
    #[arg(short = 'a', long = "agent")]
    agent: Option<String>,
    /// Do not attach to the task after creation (attach is default)
    #[arg(long = "no-attach")]
    no_attach: bool,
    /// Skip opening the editor after creating the task file
    #[arg(long)]
    no_edit: bool,
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
  Attach {
    task: Option<String>,
    #[arg(long)]
    session: Option<u64>,
  },
  /// Stop a task's sessions or a specific session
  Stop {
    task: Option<String>,
    #[arg(long)]
    session: Option<u64>,
  },
  /// List running sessions in this project
  Sessions {},
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

#[must_use]
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
    Some(Commands::New {
      slug,
      agent,
      no_attach,
      no_edit,
    }) => {
      let created = commands::new::run(&ctx, &slug, no_edit, agent.as_deref())?;
      if !no_attach {
        commands::daemon::start()?;
        let ident = created.id.to_string();
        commands::attach::run_with_task(&ctx, &ident)?;
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
    Some(Commands::Attach { task, session }) => match (task, session) {
      (Some(t), None) => commands::attach::run_with_task(&ctx, &t)?,
      (None, Some(sid)) => commands::attach::run_join_session(&ctx, sid)?,
      _ => anyhow::bail!("attach requires either a task or --session <id>"),
    },
    Some(Commands::Stop { task, session }) => {
      commands::stop::run(&ctx, task.as_deref(), session)?;
    }
    Some(Commands::Sessions {}) => {
      commands::sessions::run(&ctx)?;
    }
    None => {}
  }

  Ok(())
}
