//! Agency CLI for orchestrating the daemon and interactive PTY attach.
//!
//! Responsibilities:
//! - Parse user commands and arguments.
//! - Autostart and communicate with the JSON-RPC daemon over a Unix socket.
//! - Manage interactive PTY attach/detach with safe terminal resets.
//!
//! See `agency init` for bootstrapping `.agency` layout and project config.

pub mod args;
pub mod commands;
pub mod event_reader;
pub mod rpc;
pub mod stdin_handler;
mod term_reset;
pub mod util;

use clap::Parser;

pub fn run() {
  if std::env::args_os().len() == 1 {
    args::Cli::print_help_and_exit();
    return;
  }

  let cli = args::Cli::parse();
  match cli.command {
    Some(args::Commands::Daemon(daemon)) => match daemon.command {
      args::DaemonSubcommand::Status => {
        commands::daemon::print_status();
      }
      args::DaemonSubcommand::Start => {
        commands::daemon::start_daemon();
      }
      args::DaemonSubcommand::Stop => {
        commands::daemon::stop_daemon();
      }
      args::DaemonSubcommand::Run => {
        commands::daemon::run_daemon_foreground();
      }
      args::DaemonSubcommand::Restart => {
        commands::daemon::restart_daemon();
      }
    },
    Some(args::Commands::Init) => {
      commands::init::init_project();
    }
    Some(args::Commands::New(a)) => {
      commands::new::new_task(a);
    }
    Some(args::Commands::Start(a)) => {
      commands::start::start_task(a);
    }
    Some(args::Commands::Status) => {
      commands::status::list_status();
    }
    Some(args::Commands::Attach(a)) => {
      commands::attach::attach_interactive(a);
    }
    Some(args::Commands::Path(a)) => {
      commands::path::print_worktree_path(a);
    }
    Some(args::Commands::ShellHook) => {
      commands::shell_hook::print_shell_hook();
    }
    None => {
      args::Cli::print_help_and_exit();
    }
  }
}
