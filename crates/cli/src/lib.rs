pub mod args;
pub mod rpc;

use clap::Parser;

pub fn run() {
  // If no additional args, show help and exit 0
  if std::env::args_os().len() == 1 {
    args::Cli::print_help_and_exit();
    return;
  }

  // Parse arguments; this will also handle --help/--version.
  let cli = args::Cli::parse();
  match cli.command {
    Some(args::Commands::Daemon(daemon)) => match daemon.command {
      args::DaemonSubcommand::Status => {
        // Try to query daemon over UDS; on error print friendly message
        match orchestra_core::config::resolve_socket_path() {
          Ok(sock) => {
            let res = tokio::runtime::Builder::new_current_thread()
              .enable_io()
              .build()
              .unwrap()
              .block_on(async move { rpc::client::daemon_status(&sock).await });
            match res {
              Ok(status) => {
                // Use minimal styling; avoid unstable snapshot churn
                println!(
                  "daemon: running (v{}, pid {}, socket {})",
                  status.version, status.pid, status.socket_path
                );
              }
              Err(_) => {
                println!("daemon: stopped");
              }
            }
          }
          Err(_) => {
            println!("daemon: stopped");
          }
        }
      }
    },
    None => {
      // No subcommand provided; show help
      args::Cli::print_help_and_exit();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use clap::{CommandFactory, Parser, error::ErrorKind};

  #[test]
  fn help_flag_triggers_displayhelp() {
    // Using try_parse_from to capture the help behavior without exiting the process.
    let err = args::Cli::try_parse_from(["orchestra", "--help"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayHelp);
  }

  #[test]
  fn version_flag_triggers_displayversion() {
    let err = args::Cli::try_parse_from(["orchestra", "--version"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
  }

  #[test]
  fn command_factory_builds() {
    // Ensure the Command builder constructs without panicking
    let _ = args::Cli::command();
  }
}
