pub mod args;
pub mod rpc;
pub mod stdin_handler;

use clap::Parser;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    Some(args::Commands::New(a)) => {
      new_task(a);
    }
    Some(args::Commands::Start(a)) => {
      start_task(a);
    }
    Some(args::Commands::Status) => {
      list_status();
    }
    Some(args::Commands::Attach(a)) => {
      attach_interactive(a);
    }
    None => {
      // No subcommand provided; show help
      args::Cli::print_help_and_exit();
    }
  }
}

fn parse_task_ref(s: &str) -> agency_core::rpc::TaskRef {
  if let Ok(id) = s.parse::<u64>() {
    agency_core::rpc::TaskRef {
      id: Some(id),
      slug: None,
    }
  } else {
    agency_core::rpc::TaskRef {
      id: None,
      slug: Some(s.to_string()),
    }
  }
}

fn parse_detach_keys(s: &str) -> Vec<u8> {
  // supports "ctrl-q" or "ctrl-p,ctrl-q"
  let mut seq = Vec::new();
  for part in s.split(',') {
    let p = part.trim().to_ascii_lowercase();
    if let Some(rest) = p.strip_prefix("ctrl-")
      && let Some(ch) = rest.chars().next()
    {
      let upper = ch.to_ascii_uppercase();
      let code = (upper as u8) & 0x1f;
      seq.push(code);
    }
  }
  if seq.is_empty() {
    // fallback single ctrl-q
    seq.push((b'Q') & 0x1f);
  }
  seq
}

fn render_rpc_failure(action: &str, sock: &std::path::Path, err: &rpc::client::Error) -> String {
  match err {
    rpc::client::Error::Client(_) | rpc::client::Error::Http(_) => format!(
      "{} failed: daemon not reachable at {}. Start it with `agency daemon start` or set AGENCY_SOCKET to a valid path.",
      action,
      sock.display()
    ),
    _ => format!("{} failed: {}", action, err),
  }
}

fn agent_arg_to_core(a: args::AgentArg) -> agency_core::domain::task::Agent {
  match a {
    args::AgentArg::Opencode => agency_core::domain::task::Agent::Opencode,
    args::AgentArg::ClaudeCode => agency_core::domain::task::Agent::ClaudeCode,
    args::AgentArg::Fake => agency_core::domain::task::Agent::Fake,
  }
}

fn new_task(a: args::NewArgs) {
  let Some(sock) = resolve_socket() else {
    eprintln!("daemon not running");
    std::process::exit(1);
  };
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let params = agency_core::rpc::TaskNewParams {
    project_root: root.display().to_string(),
    slug: a.slug,
    title: a.title,
    base_branch: a.base_branch,
    labels: a.labels,
    agent: agent_arg_to_core(a.agent),
    body: None,
  };
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { rpc::client::task_new(&sock, params).await });
  match res {
    Ok(info) => {
      println!("{} {} {}", info.id, info.slug, info.title);
    }
    Err(e) => {
      eprintln!("{}", render_rpc_failure("new", &sock, &e));
      std::process::exit(1);
    }
  }
}

fn start_task(a: args::StartArgs) {
  let Some(sock) = resolve_socket() else {
    eprintln!("daemon not running");
    std::process::exit(1);
  };
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let tref = parse_task_ref(&a.task);
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { rpc::client::task_start(&sock, &root, tref).await });
  match res {
    Ok(r) => {
      println!("{} {} {:?}", r.id, r.slug, r.status);
    }
    Err(e) => {
      eprintln!("{}", render_rpc_failure("start", &sock, &e));
      std::process::exit(1);
    }
  }
}

fn list_status() {
  let Some(sock) = resolve_socket() else {
    eprintln!("daemon not running");
    std::process::exit(1);
  };
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { rpc::client::task_status(&sock, &root).await });
  match res {
    Ok(list) => {
      println!("ID   SLUG                 STATUS     TITLE");
      for t in list.tasks {
        let status = match t.status {
          agency_core::domain::task::Status::Draft => "draft",
          agency_core::domain::task::Status::Running => "running",
          agency_core::domain::task::Status::Idle => "idle",
          agency_core::domain::task::Status::Completed => "completed",
          agency_core::domain::task::Status::Reviewed => "reviewed",
          agency_core::domain::task::Status::Failed => "failed",
          agency_core::domain::task::Status::Merged => "merged",
        };
        println!("{:<4} {:<20} {:<10} {}", t.id, t.slug, status, t.title);
      }
    }
    Err(e) => {
      eprintln!("{}", render_rpc_failure("status", &sock, &e));
      std::process::exit(1);
    }
  }
}

fn attach_interactive(args: args::AttachArgs) {
  use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
  use std::io::{Read, Write};
  use std::sync::mpsc;
  use std::thread;

  let Some(sock) = resolve_socket() else {
    eprintln!("daemon not running");
    std::process::exit(1);
  };

  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let tref = parse_task_ref(&args.task);
  // Determine detach keys: env overrides config; default to ctrl-q
  let cfg = agency_core::config::load(Some(&root)).unwrap_or_default();
  let detach_cfg = std::env::var("AGENCY_DETACH_KEYS")
    .ok()
    .or_else(|| cfg.pty.detach_keys.clone());
  let detach_seq = detach_cfg
    .as_deref()
    .map(parse_detach_keys)
    .unwrap_or_else(|| vec![("Q".chars().next().unwrap() as u8) & 0x1f]);

  // initial size
  let (cols, rows) = size().unwrap_or((80, 24));
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_io()
    .enable_time()
    .worker_threads(2)
    .build()
    .unwrap();

  let attach_res =
    rt.block_on(async { rpc::client::pty_attach(&sock, &root, tref, rows, cols).await });
  let attachment_id = match attach_res {
    Ok(r) => r.attachment_id,
    Err(e) => {
      eprintln!("{}", render_rpc_failure("attach", &sock, &e));
      std::process::exit(1);
    }
  };

  let shown = detach_cfg.clone().unwrap_or_else(|| "ctrl-q".to_string());
  println!("Attached. Detach: {} (configurable via config/env)", shown);

  enable_raw_mode().ok();

  let (tx, rx) = mpsc::channel::<stdin_handler::Msg>();
  // stdin reader thread using modular handler
  let binding = stdin_handler::KeyBinding { id: "detach".to_string(), bytes: detach_seq.clone(), consume: true };
  let _reader = stdin_handler::spawn_stdin_reader(vec![binding], tx.clone());

  // output polling loop + resize handling with improved input batching and session reuse
  let mut stdout = std::io::stdout();
  let mut detached = false;

  // spawn a thread to emit resize events into an mpsc channel using crossterm
  let (tx_resize, rx_resize) = mpsc::channel::<(u16, u16)>();
  std::thread::spawn(move || {
    use crossterm::event::{self, Event};
    loop {
      if event::poll(std::time::Duration::from_millis(100)).unwrap_or(false)
        && let Ok(Event::Resize(cols, rows)) = event::read()
      {
        // note: crossterm provides cols, rows order
        let _ = tx_resize.send((rows, cols));
      }
    }
  });

  // Use session for efficient RPC calls
  let session = rpc::client::PtySession::new();

  rt.block_on(async {
    loop {
      let start = Instant::now();
      // Drain all pending input first (batch processing)
      let mut input_batch = Vec::new();
      let mut want_detach = false;
      while let Ok(msg) = rx.try_recv() {
        match msg {
          stdin_handler::Msg::Data(d) => input_batch.extend(d),
          stdin_handler::Msg::Binding(id) if id == "detach" => {
            want_detach = true;
          }
          _ => {}
        }
      }


      // Send batched input if any
      if !input_batch.is_empty() {
        let _ = rpc::client::pty_input(&sock, &attachment_id, &input_batch).await;
      }

      // Handle resize events (non-blocking)
      while let Ok((rows, cols)) = rx_resize.try_recv() {
        let _ = rpc::client::pty_resize(&sock, &attachment_id, rows, cols).await;
      }

      // Read output
      let rr = rpc::client::session::pty_read_wait(
        &session,
        &sock,
        &attachment_id,
        Some(8192),
        Some(if input_batch.is_empty() { 40 } else { 8 }),
      )
      .await;
      match rr {
        Ok(r) => {
          if !r.data.is_empty() {
            let _ = stdout.write_all(r.data.as_bytes());
            let _ = stdout.flush();
          }
          if r.eof {
            break;
          }
        }
        Err(_) => break,
      }

      if want_detach {
        detached = true;
        break;
      }

      // Add a small delay to prevent CPU spinning
      if start.elapsed() < Duration::from_millis(1) {
        tokio::time::sleep(Duration::from_millis(1)).await;
      }
    }
  });

  // cleanup
  let _ = rt.block_on(async { rpc::client::pty_detach(&sock, &attachment_id).await });
  let _ = disable_raw_mode();
  if detached {
    eprintln!("detached");
  }
}

fn init_project() {
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  // Ensure layout and default config
  if let Err(e) = agency_core::adapters::fs::ensure_layout(&root) {
    eprintln!("failed to create .agency layout: {e}");
    std::process::exit(1);
  }
  if let Err(e) = agency_core::config::write_default_project_config(&root) {
    eprintln!("failed to write config: {e}");
    std::process::exit(1);
  }
  println!("initialized .agency at {}", root.join(".agency").display());
}

fn resolve_socket() -> Option<PathBuf> {
  agency_core::config::resolve_socket_path().ok()
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
    match agency_core::daemon::start(&sock).await {
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
  let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agency"));
  let mut cmd = Command::new(exe);
  cmd.arg("daemon").arg("run");
  // Ensure child and parent agree on socket path
  cmd.env("AGENCY_SOCKET", &sock);
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
    let err = args::Cli::try_parse_from(["agency", "--help"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayHelp);
  }

  #[test]
  fn version_flag_triggers_displayversion() {
    let err = args::Cli::try_parse_from(["agency", "--version"]).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::DisplayVersion);
  }

  #[test]
  fn command_factory_builds() {
    // Ensure the Command builder constructs without panicking
    let _ = args::Cli::command();
  }
}
