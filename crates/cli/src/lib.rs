pub mod args;

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
        // Placeholder: print a deterministic status line for tests
        println!("daemon: stopped");
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
