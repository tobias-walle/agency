use std::io::{IsTerminal as _, Read as _};

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
use crate::utils::daemon::ensure_running_and_latest_version;
use crate::utils::git::resolve_main_workdir;
use crate::utils::tmux::ensure_server as ensure_tmux_server;
use crate::utils::tty::Tty;

/// Categorizes how a command uses the daemon/tmux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DaemonRequirement {
  /// Command requires daemon/tmux; fail if autostart fails
  Required,
  /// Command can use daemon but has fallback; warn on autostart failure
  Optional,
  /// Command doesn't need daemon at all; skip autostart
  None,
}

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
  /// Fast-forward merge task back to base
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
  /// Manage external CLI skills
  Skill {
    #[command(subcommand)]
    cmd: SkillCmd,
  },
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
enum SkillCmd {
  /// Install the Agency skill for external CLIs
  Install {},
}

#[derive(Debug, Subcommand)]
enum DaemonCmd {
  /// Start the daemon as a background service
  Start {},
  /// Stop the daemon gracefully
  Stop {
    /// Skip confirmation prompt and stop tmux server
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
  /// Restart the daemon (and tmux server if not running)
  Restart {
    /// Skip confirmation prompt when restarting tmux server
    #[arg(short = 'y', long = "yes")]
    yes: bool,
  },
  /// Show daemon and tmux server status
  Status {},
  /// Run the daemon in the foreground (internal)
  #[command(hide = true)]
  Run {},
}

#[derive(Debug, Subcommand)]
enum BootstrapCmd {
  /// Run bootstrap for a specific task (default when ident provided)
  Task { ident: String },
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

/// Reads description from stdin if stdin is piped (not a TTY).
/// Returns None if stdin is a TTY or if the content is empty after trimming.
fn read_description_from_stdin() -> Option<String> {
  let stdin = std::io::stdin();
  if stdin.is_terminal() {
    return None;
  }
  let mut buf = String::new();
  if stdin.lock().read_to_string(&mut buf).is_ok() {
    let trimmed = buf.trim();
    if !trimmed.is_empty() {
      return Some(trimmed.to_string());
    }
  }
  None
}

pub fn run() -> Result<()> {
  let cli = parse();
  let ctx = build_context()?;
  autostart_daemon(&ctx, cli.command.as_ref())?;
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

/// Determines whether a command requires daemon/tmux.
#[allow(clippy::match_same_arms)] // Explicit per-command for compile-time exhaustiveness
fn daemon_required(cmd: Option<&Commands>) -> DaemonRequirement {
  match cmd {
    // No command (run_default handles its own daemon logic)
    None => DaemonRequirement::None,
    // Commands that require daemon/tmux
    Some(Commands::Tui {}) => DaemonRequirement::Required,
    Some(Commands::Start { .. }) => DaemonRequirement::Required,
    Some(Commands::Attach { .. }) => DaemonRequirement::Required,
    Some(Commands::Stop { .. }) => DaemonRequirement::Required,
    Some(Commands::Sessions {}) => DaemonRequirement::Required,
    Some(Commands::Merge { .. }) => DaemonRequirement::Required,
    Some(Commands::Complete { .. }) => DaemonRequirement::Required,
    Some(Commands::Reset { .. }) => DaemonRequirement::Required,
    Some(Commands::Rm { .. }) => DaemonRequirement::Required,
    // New command only requires daemon when not a draft
    Some(Commands::New { draft: false, .. }) => DaemonRequirement::Required,
    Some(Commands::New { draft: true, .. }) => DaemonRequirement::None,
    // Commands with fallback logic
    Some(Commands::Tasks {}) => DaemonRequirement::Optional,
    Some(Commands::Fzf {}) => DaemonRequirement::Optional,
    // Commands that don't need daemon
    Some(Commands::Setup {}) => DaemonRequirement::None,
    Some(Commands::Init { .. }) => DaemonRequirement::None,
    Some(Commands::Edit { .. }) => DaemonRequirement::None,
    Some(Commands::Open { .. }) => DaemonRequirement::None,
    Some(Commands::Shell { .. }) => DaemonRequirement::None,
    Some(Commands::Exec { .. }) => DaemonRequirement::None,
    Some(Commands::Path { .. }) => DaemonRequirement::None,
    Some(Commands::Branch { .. }) => DaemonRequirement::None,
    Some(Commands::Bootstrap { .. }) => DaemonRequirement::None,
    Some(Commands::Config {}) => DaemonRequirement::None,
    Some(Commands::Defaults {}) => DaemonRequirement::None,
    Some(Commands::Gc {}) => DaemonRequirement::None,
    Some(Commands::Daemon { .. }) => DaemonRequirement::None,
    Some(Commands::Files { .. }) => DaemonRequirement::None,
    Some(Commands::Info {}) => DaemonRequirement::None,
    Some(Commands::Skill { .. }) => DaemonRequirement::None,
  }
}

/// Starts daemon/tmux if needed for the command.
///
/// # Errors
///
/// Returns an error if a required command cannot start daemon/tmux.
fn autostart_daemon(ctx: &AppContext, cmd: Option<&Commands>) -> Result<()> {
  match daemon_required(cmd) {
    DaemonRequirement::None => Ok(()),
    DaemonRequirement::Required => {
      ensure_running_and_latest_version(ctx)?;
      ensure_tmux_server(&ctx.config)?;
      Ok(())
    }
    DaemonRequirement::Optional => {
      if let Err(err) = ensure_running_and_latest_version(ctx) {
        log_warn!("Daemon autostart failed: {err}");
      }
      if let Err(err) = ensure_tmux_server(&ctx.config) {
        log_warn!("Tmux autostart failed: {err}");
      }
      Ok(())
    }
  }
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
      // Priority: positional arg > --description flag > stdin
      let desc = desc.or(description).or_else(read_description_from_stdin);
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
      (None, None) => anyhow::bail!("Bootstrap requires a task ID or slug"),
    },
    Some(Commands::Config {}) => commands::config::run(ctx),
    Some(Commands::Defaults {}) => commands::defaults::run(),
    Some(Commands::Gc {}) => commands::gc::run(ctx),
    Some(Commands::Daemon { cmd }) => match cmd {
      DaemonCmd::Start {} => commands::daemon::start(),
      DaemonCmd::Stop { yes } => commands::daemon::stop(ctx, yes),
      DaemonCmd::Restart { yes } => commands::daemon::restart(ctx, yes),
      DaemonCmd::Status {} => commands::daemon::status(ctx),
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
    Some(Commands::Skill { cmd }) => match cmd {
      SkillCmd::Install {} => commands::skill::install::run(ctx),
    },
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
  if ctx.tty.is_interactive() {
    ensure_running_and_latest_version(ctx)?;
    ensure_tmux_server(&ctx.config)?;
    tui::run(ctx)
  } else {
    log_info!("Usage: agency <SUBCOMMAND>. Try 'agency --help'");
    Ok(())
  }
}
