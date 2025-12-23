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
use crate::utils::daemon::ensure_running_and_latest_version;
use crate::utils::git::resolve_main_workdir;
use crate::utils::tmux::ensure_server as ensure_tmux_server;

/// Agency - An AI agent manager and orchestrator in your command line.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
  #[command(subcommand)]
  command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
  /// Run the setup wizard to configure Agency
  Setup {},
  /// Scaffold a .agency/ directory with starter files
  Init {
    /// Set the default agent for the project
    #[arg(short = 'a', long = "agent")]
    agent: Option<String>,
    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
  /// Interactive terminal UI
  Tui {},
  /// Create a new task under .agency/tasks
  New {
    slug: String,
    /// Optional description as a second positional
    desc: Option<String>,
    /// Select agent to attach to task (writes YAML front matter)
    #[arg(short = 'a', long = "agent")]
    agent: Option<String>,
    /// Do not start/attach after creation (start+attach is default)
    #[arg(long = "draft")]
    draft: bool,
    /// Provide description directly via flag (alias for positional)
    #[arg(long = "description")]
    description: Option<String>,
    /// Start without attaching after creation (conflicts with draft)
    #[arg(long = "no-attach", conflicts_with = "draft")]
    no_attach: bool,
    /// Open editor for description (even without --draft)
    #[arg(short = 'e', long = "edit")]
    edit: bool,
  },
  /// Open the task's markdown in $EDITOR
  Edit { ident: String },
  /// Start a task session; attach by default
  Start {
    ident: String,
    #[arg(long = "no-attach")]
    no_attach: bool,
  },
  /// Attach to an already running task session via PTY daemon
  Attach {
    task: Option<String>,
    #[arg(long)]
    session: Option<u64>,
    /// Follow the focused task in a running Agency TUI. Optional TUI id.
    /// Use without an id to auto-pick when exactly one TUI is open.
    #[arg(long = "follow", num_args(0..=1), conflicts_with = "task", conflicts_with = "session")]
    follow: Option<Option<u32>>,
  },
  /// Stop a task's sessions or a specific session
  Stop {
    task: Option<String>,
    #[arg(long)]
    session: Option<u64>,
  },
  /// Fast-forward merge task back to base and clean up
  Merge {
    ident: String,
    /// Override base branch
    #[arg(short = 'b', long = "branch")]
    base: Option<String>,
  },
  /// Mark a task as Completed (uses $`AGENCY_TASK_ID` when omitted)
  Complete { ident: Option<String> },
  /// List tasks (ID and SLUG)
  Tasks {},
  /// List running sessions in this project
  Sessions {},
  /// Open the task's worktree directory in $EDITOR
  Open { ident: String },
  /// Open a shell with the worktree as cwd
  Shell { ident: String },
  /// Execute a command in a task's worktree
  Exec {
    ident: String,
    /// Command and arguments to execute
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
    cmd: Vec<String>,
  },
  /// Print the absolute worktree path
  Path { ident: String },
  /// Print the branch name
  Branch { ident: String },
  /// Remove task file, worktree, and branch
  Rm {
    ident: String,
    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
  /// Reset a task's worktree and branch (keep markdown)
  Reset { ident: String },
  /// Prepare branch/worktree and run bootstrap (no PTY)
  Bootstrap { ident: String },
  /// Open the global config in the configured editor
  Config {},
  /// Print embedded defaults for inspection
  Defaults {},
  /// Garbage-collect orphaned branches/worktrees (no task)
  Gc {},
  /// Daemon control commands
  Daemon {
    #[command(subcommand)]
    cmd: DaemonCmd,
  },
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

// Inline overlay implemented within attach follow; dedicated Overlay subcommand removed

#[must_use]
pub fn parse() -> Cli {
  Cli::parse()
}

pub fn run() -> Result<()> {
  let cli = parse();
  let ctx = build_context()?;
  autostart_daemon(&ctx, cli.command.as_ref());
  run_command(&ctx, cli)
}

fn build_context() -> Result<AppContext> {
  let cwd = std::env::current_dir()?;
  let project_root = resolve_main_workdir(&cwd);
  let paths = AgencyPaths::new(project_root.clone());
  let config = load_config(&project_root)?;
  Ok(AppContext { paths, config })
}

fn autostart_daemon(ctx: &AppContext, cmd: Option<&Commands>) {
  if matches!(cmd, Some(Commands::Daemon { .. }) | None) {
    return;
  }
  let _ = ensure_running_and_latest_version(ctx);
  let _ = ensure_tmux_server(&ctx.config);
}

fn run_command(ctx: &AppContext, cli: Cli) -> Result<()> {
  match cli.command {
    Some(Commands::Setup {}) => commands::setup::run(ctx),
    Some(Commands::Init { agent, yes }) => commands::init::run(ctx, agent.as_deref(), yes),
    Some(Commands::Tui {}) => tui::run(ctx),
    Some(Commands::New {
      slug,
      desc,
      agent,
      draft,
      description,
      no_attach,
      edit,
    }) => {
      let desc = desc.or(description);
      let desc = if draft || edit {
        desc
      } else {
        Some(desc.unwrap_or_default())
      };
      let created = commands::new::run(ctx, &slug, agent.as_deref(), desc.as_deref(), edit)?;
      if !draft {
        let ident = created.id.to_string();
        commands::start::run_with_attach(ctx, &ident, !no_attach)?;
      }
      Ok(())
    }
    Some(Commands::Edit { ident }) => commands::edit::run(ctx, &ident),
    Some(Commands::Start { ident, no_attach }) => {
      commands::start::run_with_attach(ctx, &ident, !no_attach)
    }
    Some(Commands::Attach {
      task,
      session,
      follow,
    }) => {
      if let Some(f) = follow {
        commands::attach::run_follow(ctx, f)
      } else if let Some(t) = task {
        commands::attach::run_with_task(ctx, &t)
      } else if let Some(sid) = session {
        commands::attach::run_join_session(ctx, sid)
      } else {
        anyhow::bail!("Attach requires either a task, --session <id>, or --follow [<tui-id>]")
      }
    }
    Some(Commands::Stop { task, session }) => commands::stop::run(ctx, task.as_deref(), session),
    Some(Commands::Merge { ident, base }) => commands::merge::run(ctx, &ident, base.as_deref()),
    Some(Commands::Complete { ident }) => commands::complete::run(ctx, ident.as_deref()),
    Some(Commands::Tasks {}) => commands::tasks::run(ctx),
    Some(Commands::Sessions {}) => commands::sessions::run(ctx),
    Some(Commands::Open { ident }) => commands::open::run(ctx, &ident),
    Some(Commands::Shell { ident }) => commands::shell::run(ctx, &ident),
    Some(Commands::Exec { ident, cmd }) => {
      let code = commands::exec::run(ctx, &ident, &cmd)?;
      std::process::exit(code);
    }
    Some(Commands::Path { ident }) => commands::path::run(ctx, &ident),
    Some(Commands::Branch { ident }) => commands::branch::run(ctx, &ident),
    Some(Commands::Rm { ident, yes }) => commands::rm::run(ctx, &ident, yes),
    Some(Commands::Reset { ident }) => commands::reset::run(ctx, &ident),
    Some(Commands::Bootstrap { ident }) => commands::bootstrap::run(ctx, &ident),
    Some(Commands::Config {}) => commands::config::run(ctx),
    Some(Commands::Defaults {}) => commands::defaults::run(),
    Some(Commands::Gc {}) => commands::gc::run(ctx),
    Some(Commands::Daemon { cmd }) => match cmd {
      DaemonCmd::Start {} => commands::daemon::start(),
      DaemonCmd::Stop {} => commands::daemon::stop(),
      DaemonCmd::Restart {} => commands::daemon::restart(),
      DaemonCmd::Run {} => commands::daemon::run_blocking(),
    },
    None => run_default(ctx),
  }
}

fn run_default(ctx: &AppContext) -> Result<()> {
  let stdout_tty = std::io::stdout().is_terminal();
  if !global_config_exists() {
    if stdout_tty {
      commands::setup::run(ctx)?;
    } else {
      log_warn!("Global config missing: run `agency setup` in a terminal");
    }
    return Ok(());
  }
  let _ = ensure_running_and_latest_version(ctx);
  let _ = ensure_tmux_server(&ctx.config);
  if stdout_tty {
    tui::run(ctx)
  } else {
    log_info!("Usage: agency <SUBCOMMAND>. Try 'agency --help'");
    Ok(())
  }
}
