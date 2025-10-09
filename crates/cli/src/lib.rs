pub mod args;
pub mod rpc;
pub mod stdin_handler;
mod term_reset;

use clap::Parser;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::debug;

fn edit_text(initial: &str) -> std::io::Result<String> {
  // Resolve editor from $EDITOR or fallback to vi
  let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
  tracing::debug!(event = "cli_editor_resolved", editor = %editor, "resolved editor");

  // Create a temp file path in the system temp dir
  let mut path = std::env::temp_dir();
  let fname = format!(
    "agency-edit-{}-{}.md",
    std::process::id(),
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_millis()
  );
  path.push(fname);
  std::fs::write(&path, initial)?;

  // Launch editor inheriting stdio and env
  tracing::debug!(event = "cli_editor_launch", path = %path.display(), "launching editor");
  let status = Command::new(&editor)
    .arg(&path)
    .status()
    .map_err(|e| std::io::Error::other(format!("failed to launch editor '{}': {}", editor, e)))?;
  if !status.success() {
    return Err(std::io::Error::other(format!(
      "editor exited with status: {}",
      status
    )));
  }

  // Read edited content and cleanup
  let body = std::fs::read_to_string(&path)?;
  let _ = std::fs::remove_file(&path);
  tracing::debug!(
    event = "cli_task_body_ready",
    len = body.len(),
    "editor produced body"
  );
  Ok(body)
}

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
      args::DaemonSubcommand::Restart => {
        restart_daemon();
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
    Some(args::Commands::Path(a)) => {
      print_worktree_path(a);
    }
    Some(args::Commands::ShellHook) => {
      print_shell_hook();
    }
    None => {
      // No subcommand provided; show help
      args::Cli::print_help_and_exit();
    }
  }
}

fn print_worktree_path(a: args::PathArgs) {
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let tref = parse_task_ref(&a.task);
  // Resolve by scanning .agency/tasks without daemon
  let tasks_dir = agency_core::adapters::fs::tasks_dir(&root);
  let mut found: Option<(u64, String)> = None;
  if let Ok(rd) = std::fs::read_dir(&tasks_dir) {
    for entry in rd.flatten() {
      let name = entry.file_name();
      let name = name.to_string_lossy().to_string();
      if let Ok((tid, slug)) = agency_core::domain::task::Task::parse_filename(&name) {
        let mut ok = false;
        if let Some(id) = tref.id {
          ok = tid.0 == id;
        }
        if !ok && let Some(ref s) = tref.slug {
          ok = &slug == s;
        }
        if ok {
          found = Some((tid.0, slug));
          break;
        }
      }
    }
  }
  let (id, slug) = match found {
    Some(x) => x,
    None => {
      eprintln!("task not found");
      std::process::exit(1);
    }
  };
  let path = agency_core::adapters::fs::worktree_path(&root, id, &slug);
  println!("{}", path.display());
}

fn print_shell_hook() {
  let hook = r#"# Agency shell hook: cd into a task worktree by id or slug
agcd() {
  if [ -z "$1" ]; then
    echo "usage: agcd <id|slug>" 1>&2
    return 1
  fi
  cd "$(agency path "$1")" || return 1
}
"#;
  println!("{}", hook);
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
      "{} failed: daemon not reachable at {}.",
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

fn agent_opt_to_core(a: Option<args::AgentArg>) -> Option<agency_core::domain::task::Agent> {
  a.map(agent_arg_to_core)
}

fn ensure_daemon_running() -> PathBuf {
  let sock = match resolve_socket() {
    Some(p) => p,
    None => {
      eprintln!("could not resolve socket path");
      std::process::exit(1);
    }
  };

  // Fast path: already running
  let ok = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap()
    .block_on(async { rpc::client::daemon_status(&sock).await.is_ok() });
  if ok {
    return sock;
  }

  // Attempt background autostart silently
  let resume_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
  let _ = spawn_daemon_background(&sock, &resume_root);

  // Wait up to ~2s
  let running = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap()
    .block_on(async {
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
    sock
  } else {
    // Best-effort: return sock; callers will emit an actionable message
    sock
  }
}

fn new_task(a: args::NewArgs) {
  let sock = ensure_daemon_running();
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

  // If user did not override the built-in default and the repo uses a different default branch,
  // prefer the current HEAD branch to make start flows robust.
  let base_branch = resolve_base_branch_default(&root, &a.base_branch);

  // Resolve agent: CLI flag overrides config; else error
  let cfg = agency_core::config::load(Some(&root)).unwrap_or_default();
  let resolved_agent = agent_opt_to_core(a.agent).or(cfg.default_agent);
  if resolved_agent.is_none() {
    eprintln!("new failed: no agent specified. Provide --agent or set default_agent in config.");
    std::process::exit(2);
  }
  let agent = resolved_agent.unwrap();
  tracing::debug!(event = "cli_agent_resolved", agent = ?agent, "resolved agent for new task");

  // Collect body: use --message if provided; otherwise open editor when interactive
  let mut body_opt = a.message.clone();
  if body_opt.is_none() && std::io::stdout().is_terminal() {
    match edit_text("") {
      Ok(s) => body_opt = Some(s),
      Err(e) => {
        eprintln!("failed to capture description via editor: {}", e);
      }
    }
  }
  if let Some(ref s) = body_opt {
    tracing::debug!(
      event = "cli_task_body_ready",
      len = s.len(),
      "message provided"
    );
  }

  let params = agency_core::rpc::TaskNewParams {
    project_root: root.display().to_string(),
    slug: a.slug,
    base_branch,
    labels: a.labels,
    agent: agent.clone(),
    body: body_opt.clone(),
  };
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { rpc::client::task_new(&sock, params).await });
  match res {
    Ok(info) => {
      if a.draft {
        println!("{} {} draft", info.id, info.slug);
        return;
      }
      // Immediately start the task
      let tref = agency_core::rpc::TaskRef {
        id: Some(info.id),
        slug: None,
      };
      let start_res = rt.block_on(async { rpc::client::task_start(&sock, &root, tref).await });
      match start_res {
        Ok(sr) => {
          println!("{} {} {:?}", sr.id, sr.slug, sr.status);
          if a.no_attach {
            tracing::debug!(
              event = "cli_new_autostart_attach",
              attach = false,
              reason = "flag_no_attach",
              "skipping auto-attach by flag"
            );
            return;
          }
          if std::io::stdout().is_terminal() {
            tracing::debug!(
              event = "cli_new_autostart_attach",
              attach = true,
              reason = "stdout_tty",
              "auto-attach for new task"
            );
            let attach_args = args::AttachArgs {
              task: sr.id.to_string(),
              no_replay: false,
            };
            attach_interactive(attach_args);
          } else {
            tracing::debug!(
              event = "cli_new_autostart_attach",
              attach = false,
              reason = "stdout_not_tty",
              "stdout not a TTY; skipping auto-attach"
            );
          }
        }
        Err(e) => {
          eprintln!("{}", render_rpc_failure("start", &sock, &e));
          std::process::exit(1);
        }
      }
    }
    Err(e) => {
      eprintln!("{}", render_rpc_failure("new", &sock, &e));
      std::process::exit(1);
    }
  }
}

fn resolve_base_branch_default(root: &std::path::Path, provided: &str) -> String {
  // Only override the built-in default "main" when HEAD is on another branch.
  if provided != "main" {
    return provided.to_string();
  }
  if let Ok(repo) = git2::Repository::open(root)
    && let Ok(head) = repo.head()
    && head.is_branch()
    && let Some(name) = head.shorthand()
  {
    // Avoid empty shorthand and preserve if already "main"
    if !name.is_empty() && name != "main" {
      return name.to_string();
    }
  }
  provided.to_string()
}

fn start_task(a: args::StartArgs) {
  let sock = ensure_daemon_running();
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
  let sock = ensure_daemon_running();
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .build()
    .unwrap();
  let res = rt.block_on(async { rpc::client::task_status(&sock, &root).await });
  match res {
    Ok(list) => {
      println!("ID   SLUG                 STATUS");
      for t in list.tasks {
        let status = match t.status {
          agency_core::domain::task::Status::Draft => "draft",
          agency_core::domain::task::Status::Running => "running",
          agency_core::domain::task::Status::Stopped => "stopped",
          agency_core::domain::task::Status::Idle => "idle",
          agency_core::domain::task::Status::Completed => "completed",
          agency_core::domain::task::Status::Reviewed => "reviewed",
          agency_core::domain::task::Status::Failed => "failed",
          agency_core::domain::task::Status::Merged => "merged",
        };
        println!("{:<4} {:<20} {:<10}", t.id, t.slug, status);
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
  use std::io::Write;
  use std::sync::mpsc;

  let sock = ensure_daemon_running();

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

  let attach_res = rt.block_on(async {
    rpc::client::pty_attach_with_replay(&sock, &root, tref, rows, cols, !args.no_replay).await
  });
  let attachment_id = match attach_res {
    Ok(r) => r.attachment_id,
    Err(e) => {
      eprintln!("{}", render_rpc_failure("attach", &sock, &e));
      std::process::exit(1);
    }
  };
  debug!(event = "cli_attach_ok", %attachment_id, rows, cols, "attached to PTY");

  let shown = detach_cfg.clone().unwrap_or_else(|| "ctrl-q".to_string());
  debug!(event = "cli_detach_keys", shown = %shown, seq = ?detach_seq, "detach keys resolved");
  println!("Attached. Detach: {} (configurable via config/env)", shown);

  enable_raw_mode().ok();

  let (tx, rx) = mpsc::channel::<stdin_handler::Msg>();
  // stdin reader thread using modular handler
  let binding = stdin_handler::KeyBinding {
    id: "detach".to_string(),
    bytes: detach_seq.clone(),
    consume: true,
  };
  let _reader = stdin_handler::spawn_stdin_reader(vec![binding], tx.clone());

  // output polling loop + resize handling with improved input batching and session reuse
  let mut stdout = std::io::stdout();
  let mut detached = false;

  // spawn a thread to poll terminal size without consuming stdin events
  let (tx_resize, rx_resize) = mpsc::channel::<(u16, u16)>();
  std::thread::spawn(move || {
    use crossterm::terminal::size;
    let mut last_rows: u16 = 0;
    let mut last_cols: u16 = 0;
    loop {
      if let Ok((cols, rows)) = size()
        && (rows != last_rows || cols != last_cols)
      {
        last_rows = rows;
        last_cols = cols;
        let _ = tx_resize.send((rows, cols));
      }
      std::thread::sleep(std::time::Duration::from_millis(200));
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
      let mut drained_msgs = 0usize;
      let mut drained_bytes = 0usize;
      let mut drained_bindings = 0usize;
      while let Ok(msg) = rx.try_recv() {
        match msg {
          stdin_handler::Msg::Data(d) => {
            let len = d.len();
            input_batch.extend(d);
            drained_msgs += 1;
            drained_bytes += len;
          }
          stdin_handler::Msg::Binding(id) if id == "detach" => {
            want_detach = true;
            drained_bindings += 1;
          }
          _ => {}
        }
      }
      if drained_msgs > 0 || drained_bindings > 0 {
        debug!(
          event = "cli_stdin_drained",
          drained_msgs,
          drained_bytes,
          drained_bindings,
          batch_len = input_batch.len(),
          "drained stdin messages"
        );
      }

      // Send batched input if any
      if !input_batch.is_empty() {
        debug!(
          event = "cli_pty_input_send",
          n = input_batch.len(),
          "sending input batch"
        );
        let _ = rpc::client::pty_input(&sock, &attachment_id, &input_batch).await;
      }

      // Handle resize events (non-blocking)
      while let Ok((rows, cols)) = rx_resize.try_recv() {
        debug!(event = "cli_pty_resize_send", rows, cols, "sending resize");
        let _ = rpc::client::pty_resize(&sock, &attachment_id, rows, cols).await;
      }

      // Read output
      let wait_ms = if input_batch.is_empty() { 40 } else { 8 };
      let rr = rpc::client::session::pty_read_wait(
        &session,
        &sock,
        &attachment_id,
        Some(8192),
        Some(wait_ms),
      )
      .await;
      match rr {
        Ok(r) => {
          debug!(
            event = "cli_pty_read_result",
            bytes = r.data.len(),
            eof = r.eof,
            wait_ms,
            "read from PTY"
          );
          if !r.data.is_empty() {
            let _ = stdout.write_all(r.data.as_bytes());
            let _ = stdout.flush();
          }
          if r.eof {
            break;
          }
        }
        Err(e) => {
          debug!(event = "cli_pty_read_error", error = %e, "read error");
          break;
        }
      }

      if want_detach {
        debug!(event = "cli_detach_requested", "detach binding triggered");
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
  debug!(event = "cli_pty_detach_send", %attachment_id, "sending detach");
  let _ = rt.block_on(async { rpc::client::pty_detach(&sock, &attachment_id).await });
  let _ = disable_raw_mode();
  // Emit terminal reset footer if stdout is a TTY (or test override)
  let force = std::env::var("AGENCY_FORCE_TTY_RESET").ok().is_some();
  if force || std::io::stdout().is_terminal() {
    let mut out = std::io::stdout();
    let _ = term_reset::write_reset_footer(&mut out);
  }
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

fn spawn_daemon_background(sock: &Path, resume_root: &Path) -> std::io::Result<()> {
  let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("agency"));
  let mut cmd = Command::new(exe);
  cmd.arg("daemon").arg("run");
  // Ensure child and parent agree on socket path and resume root
  cmd.env("AGENCY_SOCKET", sock);
  cmd.env("AGENCY_RESUME_ROOT", resume_root);
  // Detach stdio
  cmd
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
  let _ = cmd.spawn()?;
  Ok(())
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
  let resume_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
  if spawn_daemon_background(&sock, &resume_root).is_err() {
    println!("daemon: stopped");
    return;
  }

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

fn restart_daemon() {
  // Best-effort stop then start
  stop_daemon();
  start_daemon();
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
