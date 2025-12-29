use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
pub mod config;
pub mod daemon;
pub mod daemon_protocol;
mod texts;
pub mod tui;
mod utils;

use crate::config::{AgencyPaths, AppContext, global_config_exists, load_config};
use crate::utils::tty::Tty;
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
    /// Attach file(s) to the task (can be repeated)
    #[arg(short = 'f', long = "file")]
    files: Vec<String>,
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
  /// Merge task into base and clean up (branch, worktree, file)
  Complete {
    ident: Option<String>,
    /// Override base branch
    #[arg(short = 'b', long = "branch")]
    base: Option<String>,
    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
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
  /// Select a task with fzf and output its ID
  Fzf {},
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
  Bootstrap {
    #[command(subcommand)]
    cmd: Option<BootstrapCmd>,
    /// Task ID or slug (for default subcommand)
    ident: Option<String>,
  },
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
  /// Manage files attached to a task
  Files {
    #[command(subcommand)]
    cmd: FilesCmd,
  },
  /// Show current task context and attached files
  Info {},
}

#[derive(Debug, Subcommand)]
enum DaemonCmd {
  /// Start the daemon as a background service
  Start {},
  /// Stop the daemon gracefully
  Stop {},
  /// Restart the daemon (and tmux server if not running)
  Restart {
    /// Skip confirmation prompt when restarting tmux server
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
  /// Run the daemon in the foreground (internal)
  #[command(hide = true)]
  Run {},
}

#[derive(Debug, Subcommand)]
enum BootstrapCmd {
  /// Run bootstrap for a specific task (default when ident provided)
  Task { ident: String },
  /// Internal: run bootstrap in worktree (called by daemon)
  #[command(hide = true)]
  Run {
    #[arg(long)]
    repo_root: String,
    #[arg(long)]
    worktree_dir: String,
    #[arg(long = "include", action = clap::ArgAction::Append)]
    include: Vec<String>,
    #[arg(long = "exclude", action = clap::ArgAction::Append)]
    exclude: Vec<String>,
    #[arg(long = "cmd", action = clap::ArgAction::Append)]
    cmd: Vec<String>,
  },
}

#[derive(Debug, Subcommand)]
#[allow(clippy::struct_field_names)]
enum FilesCmd {
  /// List files attached to a task
  List {
    /// Task ID or slug
    task: String,
  },
  /// Add a file to a task
  Add {
    /// Task ID or slug
    task: String,
    /// Path to the source file
    source: Option<String>,
    /// Read image from clipboard (optionally specify filename)
    #[arg(long = "from-clipboard", num_args = 0..=1, default_missing_value = "clipboard.png")]
    from_clipboard: Option<String>,
  },
  /// Remove a file from a task
  Rm {
    /// Task ID or slug
    task: String,
    /// File ID or name
    file: String,
    /// Skip confirmation prompt
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
  /// Print the path to a file or files directory
  Path {
    /// Task ID or slug
    task: String,
    /// File ID or name (omit to print directory)
    file: Option<String>,
  },
  /// Select a file with fzf
  Fzf {
    /// Task ID or slug
    task: String,
  },
  /// Open a file or the files directory
  Open {
    /// Task ID or slug
    task: String,
    /// File ID or name (omit to open directory)
    file: Option<String>,
  },
  /// Edit a file in $EDITOR
  Edit {
    /// Task ID or slug
    task: String,
    /// File ID or name
    file: String,
  },
}

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
  let paths = AgencyPaths::new(project_root.clone(), cwd);
  let config = load_config(&project_root)?;
  let tty = Tty::new();
  Ok(AppContext { paths, config, tty })
}

fn autostart_daemon(ctx: &AppContext, cmd: Option<&Commands>) {
  if matches!(cmd, Some(Commands::Daemon { .. }) | None) {
    return;
  }
  let _ = ensure_running_and_latest_version(ctx);
  let _ = ensure_tmux_server(&ctx.config);
}

#[allow(clippy::too_many_lines)]
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
      files,
    }) => {
      let desc = desc.or(description);
      let desc = if draft || edit {
        desc
      } else {
        Some(desc.unwrap_or_default())
      };
      let created =
        commands::new::run(ctx, &slug, agent.as_deref(), desc.as_deref(), edit, &files)?;
      if !draft {
        let ident = created.id.to_string();
        // Only attach in interactive mode; non-interactive defaults to no-attach
        let should_attach = !no_attach && ctx.tty.is_interactive();
        commands::start::run_with_attach(ctx, &ident, should_attach)?;
      }
      Ok(())
    }
    Some(Commands::Edit { ident }) => commands::edit::run(ctx, &ident),
    Some(Commands::Start { ident, no_attach }) => {
      // Only attach in interactive mode; non-interactive defaults to no-attach
      let should_attach = !no_attach && ctx.tty.is_interactive();
      commands::start::run_with_attach(ctx, &ident, should_attach)
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
    Some(Commands::Complete { ident, base, yes }) => {
      commands::complete::run(ctx, ident.as_deref(), base.as_deref(), yes)
    }
    Some(Commands::Tasks {}) => commands::tasks::run(ctx),
    Some(Commands::Sessions {}) => commands::sessions::run(ctx),
    Some(Commands::Open { ident }) => commands::open::run(ctx, &ident),
    Some(Commands::Shell { ident }) => commands::shell::run(ctx, &ident),
    Some(Commands::Exec { ident, cmd }) => {
      let code = commands::exec::run(ctx, &ident, &cmd)?;
      std::process::exit(code);
    }
    Some(Commands::Fzf {}) => commands::fzf::run(ctx),
    Some(Commands::Path { ident }) => commands::path::run(ctx, &ident),
    Some(Commands::Branch { ident }) => commands::branch::run(ctx, &ident),
    Some(Commands::Rm { ident, yes }) => commands::rm::run(ctx, &ident, yes),
    Some(Commands::Reset { ident }) => commands::reset::run(ctx, &ident),
    Some(Commands::Bootstrap { cmd, ident }) => match (cmd, ident) {
      (Some(BootstrapCmd::Task { ident }), _) | (None, Some(ident)) => {
        commands::bootstrap::run(ctx, &ident)
      }
      (Some(BootstrapCmd::Run { repo_root, worktree_dir, include, exclude, cmd }), _) => {
        commands::bootstrap::run_internal(&repo_root, &worktree_dir, &include, &exclude, &cmd)
      }
      (None, None) => anyhow::bail!("Bootstrap requires a task ID or slug"),
    },
    Some(Commands::Config {}) => commands::config::run(ctx),
    Some(Commands::Defaults {}) => commands::defaults::run(),
    Some(Commands::Gc {}) => commands::gc::run(ctx),
    Some(Commands::Daemon { cmd }) => match cmd {
      DaemonCmd::Start {} => commands::daemon::start(),
      DaemonCmd::Stop {} => commands::daemon::stop(),
      DaemonCmd::Restart { yes } => commands::daemon::restart(ctx, yes),
      DaemonCmd::Run {} => commands::daemon::run_blocking(),
    },
    Some(Commands::Files { cmd }) => match cmd {
      FilesCmd::List { task } => commands::files::list::run(ctx, &task),
      FilesCmd::Add {
        task,
        source,
        from_clipboard,
      } => commands::files::add::run(ctx, &task, source.as_deref(), from_clipboard.as_deref()),
      FilesCmd::Rm { task, file, yes } => commands::files::rm::run(ctx, &task, &file, yes),
      FilesCmd::Path { task, file } => commands::files::path::run(ctx, &task, file.as_deref()),
      FilesCmd::Fzf { task } => commands::files::fzf::run(ctx, &task),
      FilesCmd::Open { task, file } => commands::files::open::run(ctx, &task, file.as_deref()),
      FilesCmd::Edit { task, file } => commands::files::edit::run(ctx, &task, &file),
    },
    Some(Commands::Info {}) => commands::info::run(ctx),
    None => run_default(ctx),
  }
}

fn run_default(ctx: &AppContext) -> Result<()> {
  if !global_config_exists() {
    if ctx.tty.is_interactive() {
      commands::setup::run(ctx)?;
    } else {
      log_warn!("Global config missing: run `agency setup` in a terminal");
    }
    return Ok(());
  }
  let _ = ensure_running_and_latest_version(ctx);
  let _ = ensure_tmux_server(&ctx.config);
  if ctx.tty.is_interactive() {
    tui::run(ctx)
  } else {
    log_info!("Usage: agency <SUBCOMMAND>. Try 'agency --help'");
    Ok(())
  }
}
