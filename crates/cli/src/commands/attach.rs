use crate::{
  args,
  rpc::client,
  stdin_handler, term_reset,
  util::{
    daemon_proc::ensure_daemon_running, detach_keys::parse_detach_keys, task_ref::parse_task_ref,
  },
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use std::io::IsTerminal;
use std::io::Write;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::debug;

pub fn attach_interactive(args: args::AttachArgs) {
  let sock = ensure_daemon_running();
  let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
  let tref = parse_task_ref(&args.task);
  let cfg = agency_core::config::load(Some(&root)).unwrap_or_default();
  let detach_cfg = std::env::var("AGENCY_DETACH_KEYS")
    .ok()
    .or_else(|| cfg.pty.detach_keys.clone());
  let detach_seq = detach_cfg
    .as_deref()
    .map(parse_detach_keys)
    .unwrap_or_else(|| vec![("Q".chars().next().unwrap() as u8) & 0x1f]);

  let (cols, rows) = size().unwrap_or((80, 24));
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_io()
    .enable_time()
    .worker_threads(2)
    .build()
    .unwrap();

  let attach_res = rt.block_on(async {
    client::pty_attach_with_replay(&sock, &root, tref, rows, cols, !args.no_replay).await
  });
  let attachment_id = match attach_res {
    Ok(r) => r.attachment_id,
    Err(e) => {
      eprintln!(
        "{}",
        crate::util::errors::render_rpc_failure("attach", &sock, &e)
      );
      std::process::exit(1);
    }
  };
  debug!(event = "cli_attach_ok", %attachment_id, rows, cols, "attached to PTY");

  let shown = detach_cfg.clone().unwrap_or_else(|| "ctrl-q".to_string());
  debug!(event = "cli_detach_keys", shown = %shown, seq = ?detach_seq, "detach keys resolved");
  println!("Attached. Detach: {} (configurable via config/env)", shown);

  enable_raw_mode().ok();

  let (tx, rx) = mpsc::channel::<stdin_handler::Msg>();
  let binding = stdin_handler::KeyBinding {
    id: "detach".to_string(),
    bytes: detach_seq.clone(),
    consume: true,
  };
  let _reader = stdin_handler::spawn_stdin_reader(vec![binding], tx.clone());

  let mut stdout = std::io::stdout();
  let mut detached = false;

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

  let session = client::PtySession::new();

  rt.block_on(async {
    loop {
      let start = Instant::now();
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

      if !input_batch.is_empty() {
        debug!(
          event = "cli_pty_input_send",
          n = input_batch.len(),
          "sending input batch"
        );
        let _ = client::pty_input(&sock, &attachment_id, &input_batch).await;
      }

      while let Ok((rows, cols)) = rx_resize.try_recv() {
        debug!(event = "cli_pty_resize_send", rows, cols, "sending resize");
        let _ = client::pty_resize(&sock, &attachment_id, rows, cols).await;
      }

      let wait_ms = if input_batch.is_empty() { 40 } else { 8 };
      let rr =
        client::session::pty_read_wait(&session, &sock, &attachment_id, Some(8192), Some(wait_ms))
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

      if start.elapsed() < Duration::from_millis(1) {
        tokio::time::sleep(Duration::from_millis(1)).await;
      }
    }
  });

  debug!(event = "cli_pty_detach_send", %attachment_id, "sending detach");
  let _ = rt.block_on(async { client::pty_detach(&sock, &attachment_id).await });
  let _ = disable_raw_mode();
  let force = std::env::var("AGENCY_FORCE_TTY_RESET").ok().is_some();
  if force || std::io::stdout().is_terminal() {
    let mut out = std::io::stdout();
    let _ = term_reset::write_reset_footer(&mut out);
  }
  if detached {
    eprintln!("detached");
  }
}
