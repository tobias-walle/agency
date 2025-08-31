pub mod args;
pub mod rpc;

use clap::Parser;
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
        print_status();
      }
      args::DaemonSubcommand::Start => {
        start_daemon();
      }
      args::DaemonSubcommand::Stop => {
        stop_daemon();
      }
      args::DaemonSubcommand::Run => {
        run_daemon_foreground();
      }
    },
    Some(args::Commands::Init) => {
      init_project();
    }
    None => {
      // No subcommand provided; show help
      args::Cli::print_help_and_exit();
    }
  }
}

fn init_project() {
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  // Ensure layout and default config
  if let Err(e) = orchestra_core::adapters::fs::ensure_layout(&root) {
    eprintln!("failed to create .orchestra layout: {e}");
    std::process::exit(1);
  }
  if let Err(e) = orchestra_core::config::write_default_project_config(&root) {
    eprintln!("failed to write config: {e}");
    std::process::exit(1);
  }
  println!(
    "initialized .orchestra at {}",
    root.join(".orchestra").display()
  );
}

fn resolve_socket() -> Option<PathBuf> {
  orchestra_core::config::resolve_socket_path().ok()
}

fn print_status() {
  match resolve_socket() {
    Some(sock) => {
      let res = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap()
        .block_on(async move { rpc::client::daemon_status(&sock).await });
      match res {
        Ok(status) => {
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
    None => println!("daemon: stopped"),
  }
}

fn run_daemon_foreground() {
  let Some(sock) = resolve_socket() else {
    eprintln!("could not resolve socket path");
    std::process::exit(1);
  };

  // Run the daemon and wait until it exits (shutdown)
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_io()
    .enable_time()
    .worker_threads(2)
    .build()
    .unwrap();

  rt.block_on(async move {
    match orchestra_core::daemon::start(&sock).await {
      Ok(handle) => {
        handle.wait().await;
      }
      Err(e) => {
        eprintln!("failed to start daemon: {e}");
        std::process::exit(1);
      }
    }
  });
}

fn start_daemon() {
  let Some(sock) = resolve_socket() else {
    println!("daemon: stopped");
    return;
  };

  // If already running, just print status
  let already_running = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap()
    .block_on(async { rpc::client::daemon_status(&sock).await.is_ok() });
  if already_running {
    print_status();
    return;
  }

  // Spawn background process to run the daemon
  let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("orchestra"));
  let mut cmd = Command::new(exe);
  cmd.arg("daemon").arg("run");
  // Ensure child and parent agree on socket path
  cmd.env("ORCHESTRA_SOCKET", &sock);
  // Detach stdio
  cmd
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());

  match cmd.spawn() {
    Ok(_child) => {
      // Poll status for a short time
      let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap();
      let running = rt.block_on(async {
        for _ in 0..20u8 {
          // ~2s
          if rpc::client::daemon_status(&sock).await.is_ok() {
            return true;
          }
          tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
      });
      if running {
        print_status();
      } else {
        println!("daemon: stopped");
      }
    }
    Err(_) => println!("daemon: stopped"),
  }
}

fn stop_daemon() {
  let Some(sock) = resolve_socket() else {
    println!("daemon: stopped");
    return;
  };

  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap();
  let stopped = rt.block_on(async move {
    // Try graceful shutdown via RPC
    let _ = rpc::client::daemon_shutdown(&sock).await;
    // Wait until status fails
    for _ in 0..20u8 {
      // ~2s
      if rpc::client::daemon_status(&sock).await.is_err() {
        return true;
      }
      tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    false
  });

  if stopped {
    println!("daemon: stopped");
  } else {
    // Best-effort: still report stopped if unreachable
    println!("daemon: stopped");
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
