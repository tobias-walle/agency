use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::IsTerminal as _;

mod commands;
pub mod config;
pub mod daemon;
pub mod daemon_protocol;
mod texts;
pub mod tui;
mod utils;

use crate::config::{AgencyPaths, AppContext, global_config_exists, load_config};
use crate::utils::git::resolve_main_workdir;

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
    /// Do not start/attach after creation (start+attach is default)
    #[arg(long = "draft")]
    draft: bool,
    /// Skip opening the editor after creating the task file
    #[arg(long)]
    no_edit: bool,
  },
  /// Fast-forward merge task back to base and clean up
  Merge {
    ident: String,
    /// Override base branch
    #[arg(short = 'b', long = "branch")]
    base: Option<String>,
  },
  /// Open the task's worktree directory in $EDITOR
  Open { ident: String },
  /// Open the task's markdown in $EDITOR
  Edit { ident: String },
  /// Open a shell with the worktree as cwd
  Shell { ident: String },
  /// Print the absolute worktree path
  Path { ident: String },
  /// Print the branch name
  Branch { ident: String },
  /// Remove task file, worktree, and branch
  Rm { ident: String },
  /// Reset a task's worktree and branch (keep markdown)
  Reset { ident: String },
  /// List tasks (ID and SLUG)
  Ps {},
  /// Daemon control commands
  Daemon {
    #[command(subcommand)]
    cmd: DaemonCmd,
  },
  /// Attach to an already running task session via PTY daemon
  Attach {
    task: Option<String>,
    #[arg(long)]
    session: Option<u64>,
  },
  /// Start a task session and attach; fails if already started
  Start { ident: String },
  /// Prepare branch/worktree and run bootstrap (no PTY)
  Bootstrap { ident: String },
  /// Stop a task's sessions or a specific session
  Stop {
    task: Option<String>,
    #[arg(long)]
    session: Option<u64>,
  },
  /// List running sessions in this project
  Sessions {},
  /// Mark a task as Completed (uses $`AGENCY_TASK_ID` when omitted)
  Complete { ident: Option<String> },
  /// Run the setup wizard to configure Agency
  Setup {},
  /// Print embedded defaults for inspection
  Defaults {},
  /// Scaffold a .agency/ directory with starter files
  Init {},
  /// Interactive terminal UI
  Tui {},
  /// Garbage-collect orphaned branches/worktrees (no task)
  Gc {},
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
  let project_root = resolve_main_workdir(&cwd);
  let paths = AgencyPaths::new(project_root.clone());
  let config = load_config(&project_root)?;
  let ctx = AppContext { paths, config };

  match cli.command {
    Some(Commands::New {
      slug,
      agent,
      draft,
      no_edit,
    }) => {
      let created = commands::new::run(&ctx, &slug, no_edit, agent.as_deref())?;
      if !draft {
        let ident = created.id.to_string();
        // Start the session and attach; fails if already started
        commands::start::run(&ctx, &ident)?;
      }
    }
    Some(Commands::Merge { ident, base }) => {
      commands::merge::run(&ctx, &ident, base.as_deref())?;
    }
    Some(Commands::Open { ident }) => {
      commands::open::run(&ctx, &ident)?;
    }
    Some(Commands::Edit { ident }) => {
      commands::edit::run(&ctx, &ident)?;
    }
    Some(Commands::Shell { ident }) => {
      commands::shell::run(&ctx, &ident)?;
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
    Some(Commands::Reset { ident }) => {
      commands::reset::run(&ctx, &ident)?;
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
      _ => anyhow::bail!("Attach requires either a task or --session <id>"),
    },
    Some(Commands::Bootstrap { ident }) => {
      commands::bootstrap::run(&ctx, &ident)?;
    }
    Some(Commands::Start { ident }) => {
      commands::start::run(&ctx, &ident)?;
    }
    Some(Commands::Stop { task, session }) => {
      commands::stop::run(&ctx, task.as_deref(), session)?;
    }
    Some(Commands::Sessions {}) => {
      commands::sessions::run(&ctx)?;
    }
    Some(Commands::Complete { ident }) => {
      commands::complete::run(&ctx, ident.as_deref())?;
    }
    Some(Commands::Setup {}) => {
      commands::setup::run(&ctx)?;
    }
    Some(Commands::Defaults {}) => {
      commands::defaults::run()?;
    }
    Some(Commands::Init {}) => {
      commands::init::run(&ctx)?;
    }
    Some(Commands::Tui {}) => {
      crate::tui::run(&ctx)?;
    }
    Some(Commands::Gc {}) => {
      commands::gc::run(&ctx)?;
    }
    None => {
      let stdout_tty = std::io::stdout().is_terminal();
      if !global_config_exists() {
        if stdout_tty {
          commands::setup::run(&ctx)?;
        } else {
          crate::log_warn!("Global config missing: run `agency setup` in a terminal");
        }
        return Ok(());
      }
      if stdout_tty {
        crate::tui::run(&ctx)?;
      } else {
        crate::log_info!("Usage: agency <SUBCOMMAND>. Try 'agency --help'");
      }
    }
  }

  Ok(())
}
