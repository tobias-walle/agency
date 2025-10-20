use crate::{
  args, event_reader,
  rpc::client,
  term_reset,
  util::{
    daemon_proc::ensure_daemon_running, detach_keys::parse_detach_keys, task_ref::parse_task_ref,
  },
};
use crossterm::{
  event::EnableBracketedPaste,
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, size},
};
use std::io::IsTerminal;
use std::io::Read;
use std::io::Write;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracing::debug;

pub fn attach_interactive(args: args::AttachArgs) {
  // Enforce TTY-only attach per crokey-driven input unless explicitly allowed for tests/pipes
  let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
  if !is_tty {
    debug!(event = "cli_attach_non_tty", "stdin/stdout are not TTY; using fallback");
  }

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
    .unwrap_or_else(|| {
      vec![crokey::key!(ctrl-q)]
    });

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
  let crokey_shown = detach_seq
    .iter()
    .map(|kc| format!("{}", kc))
    .collect::<Vec<_>>()
    .join(",");
  debug!(event = "cli_detach_keys", shown = %shown, crokey = %crokey_shown, seq_len = detach_seq.len(), "detach keys resolved");
  println!("Attached. Detach: {} (configurable via config/env)", shown);

  let session = client::PtySession::new();

  if is_tty {
    // ---- TTY path: use crokey combiner and bracketed paste ----
    enable_raw_mode().ok();
    // Enable bracketed paste for the session; disable on teardown via term_reset
    let _ = execute!(std::io::stdout(), EnableBracketedPaste);

    let (tx, rx) = mpsc::channel::<event_reader::EventMsg>();
    let _reader = event_reader::spawn_event_reader(tx.clone());

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

    // Simple sequence matcher buffer for detach (Crokey combinations)
    let mut seq_buf: Vec<crokey::KeyCombination> = Vec::new();

    fn dto_for_code(
      code: crossterm::event::KeyCode,
      modifiers: agency_core::rpc::ModifiersDTO,
    ) -> agency_core::rpc::KeyCombinationDTO {
      use agency_core::rpc::KeyCodeDTO;
      let dto_code = match code {
        crossterm::event::KeyCode::Char(c) => KeyCodeDTO::Char(c),
        crossterm::event::KeyCode::Enter => KeyCodeDTO::Enter,
        crossterm::event::KeyCode::Backspace => KeyCodeDTO::Backspace,
        crossterm::event::KeyCode::Tab => KeyCodeDTO::Tab,
        crossterm::event::KeyCode::Up => KeyCodeDTO::Up,
        crossterm::event::KeyCode::Down => KeyCodeDTO::Down,
        crossterm::event::KeyCode::Left => KeyCodeDTO::Left,
        crossterm::event::KeyCode::Right => KeyCodeDTO::Right,
        crossterm::event::KeyCode::Home => KeyCodeDTO::Home,
        crossterm::event::KeyCode::End => KeyCodeDTO::End,
        crossterm::event::KeyCode::PageUp => KeyCodeDTO::PageUp,
        crossterm::event::KeyCode::PageDown => KeyCodeDTO::PageDown,
        crossterm::event::KeyCode::F(n) => KeyCodeDTO::F(n),
        _ => KeyCodeDTO::Char('?'),
      };
      agency_core::rpc::KeyCombinationDTO { code: dto_code, modifiers }
    }

    fn combos_to_dtos(kc: &crokey::KeyCombination) -> Vec<agency_core::rpc::KeyCombinationDTO> {
      use crossterm::event::KeyModifiers;
      let modifiers = agency_core::rpc::ModifiersDTO {
        ctrl: kc.modifiers.contains(KeyModifiers::CONTROL),
        alt: kc.modifiers.contains(KeyModifiers::ALT),
        shift: kc.modifiers.contains(KeyModifiers::SHIFT),
      };
      match kc.codes {
        crokey::OneToThree::One(a) => vec![dto_for_code(a, modifiers.clone())],
        crokey::OneToThree::Two(a, b) => {
          vec![dto_for_code(a, modifiers.clone()), dto_for_code(b, modifiers.clone())]
        }
        crokey::OneToThree::Three(a, b, c) => vec![
          dto_for_code(a, modifiers.clone()),
          dto_for_code(b, modifiers.clone()),
          dto_for_code(c, modifiers.clone()),
        ],
      }
    }

    rt.block_on(async {
      loop {
        let start = Instant::now();
        let mut events_batch: Vec<agency_core::rpc::KeyCombinationDTO> = Vec::new();
        let mut want_detach = false;
        let mut drained_msgs = 0usize;
        while let Ok(msg) = rx.try_recv() {
          match msg {
            event_reader::EventMsg::Combo(kc) => {
              drained_msgs += 1;
              seq_buf.push(kc);
              // Keep only last N combos where N = detach_seq.len()
              if seq_buf.len() > detach_seq.len() {
                let overflow = seq_buf.len() - detach_seq.len();
                seq_buf.drain(0..overflow);
              }
              // Check suffix match
              if !detach_seq.is_empty()
                && seq_buf.len() == detach_seq.len()
                && seq_buf.iter().zip(detach_seq.iter()).all(|(a, b)| a == b)
              {
                want_detach = true;
                // Do not forward detach events to PTY
                seq_buf.clear();
                continue;
              }
              // Non-detach: forward, converting to DTO(s)
              events_batch.extend(combos_to_dtos(&kc));
            }
            event_reader::EventMsg::Paste(s) => {
              drained_msgs += 1;
              debug!(event = "cli_paste", bytes = s.len(), "forwarding paste payload");
              let _ = client::session::pty_input(&session, &sock, &attachment_id, s.as_bytes()).await;
            }
          }
        }
        if drained_msgs > 0 {
          debug!(
            event = "cli_events_drained",
            drained_msgs,
            batch_len = events_batch.len()
          );
        }

        if !events_batch.is_empty() {
          debug!(
            event = "cli_pty_input_events_send",
            n = events_batch.len(),
            "sending events batch"
          );
          let _ = client::session::pty_input_events(&session, &sock, &attachment_id, &events_batch)
            .await;
        }

        // Read outgoing PTY data
        let wait_ms = if events_batch.is_empty() { 40 } else { 8 };
        let rr = client::session::pty_read_wait(&session, &sock, &attachment_id, Some(8192), Some(wait_ms))
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
          debug!(event = "cli_detach_requested", "detach sequence triggered");
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
    // Print detach marker for tests
    eprintln!("detached");
    return;
  }

  // ---- Non-TTY path: read raw bytes from stdin; detect ctrl-letter detach; forward bytes ----
  fn ctrl_byte_for_letter(ch: char) -> Option<u8> {
    let u = ch.to_ascii_uppercase() as u8;
    if (b'A'..=b'Z').contains(&u) {
      Some(u & 0x1F)
    } else {
      None
    }
  }

  fn detach_bytes_from_seq(seq: &[crokey::KeyCombination]) -> Vec<u8> {
    let mut out = Vec::new();
    for kc in seq {
      if let Some(ch) = kc.as_letter() {
        if kc.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
          if let Some(b) = ctrl_byte_for_letter(ch) {
            out.push(b);
          }
        }
      }
    }
    out
  }

  let detach_bytes = detach_bytes_from_seq(&detach_seq);
  debug!(event = "cli_detach_bytes", len = detach_bytes.len(), "byte-oriented detach sequence computed");

  let (tx_bytes, rx_bytes) = mpsc::channel::<Vec<u8>>();
  std::thread::spawn(move || {
    let mut stdin = std::io::stdin();
    let mut buf = [0u8; 1024];
    loop {
      match stdin.read(&mut buf) {
        Ok(0) => break,
        Ok(n) => {
          let _ = tx_bytes.send(buf[..n].to_vec());
        }
        Err(_) => break,
      }
    }
  });

  let mut stdout = std::io::stdout();
  let mut detached = false;
  let mut seq_buf: Vec<u8> = Vec::new();

  rt.block_on(async {
    loop {
      let start = Instant::now();
      let mut bytes_batch: Vec<u8> = Vec::new();
      let mut drained = 0usize;
      while let Ok(chunk) = rx_bytes.try_recv() {
        drained += 1;
        for &b in &chunk {
          seq_buf.push(b);
          if seq_buf.len() > detach_bytes.len() {
            let overflow = seq_buf.len() - detach_bytes.len();
            seq_buf.drain(0..overflow);
          }
          if !detach_bytes.is_empty()
            && seq_buf.len() == detach_bytes.len()
            && seq_buf.iter().zip(detach_bytes.iter()).all(|(a, c)| a == c)
          {
            debug!(event = "cli_detach_requested_bytes", "detach sequence triggered (bytes)");
            bytes_batch.clear();
            seq_buf.clear();
            detached = true;
            break;
          }
          // Not part of a matched detach suffix: forward
          bytes_batch.push(b);
        }
        if detached {
          break;
        }
      }

      if drained > 0 {
        debug!(event = "cli_bytes_drained", drained, batch_len = bytes_batch.len());
      }

      if !bytes_batch.is_empty() {
        let _ = client::session::pty_input(&session, &sock, &attachment_id, &bytes_batch).await;
      }

      let wait_ms = if bytes_batch.is_empty() { 40 } else { 8 };
      let rr = client::session::pty_read_wait(&session, &sock, &attachment_id, Some(8192), Some(wait_ms))
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
        Err(e) => {
          debug!(event = "cli_pty_read_error", error = %e, "read error");
          break;
        }
      }

      if detached {
        break;
      }

      if start.elapsed() < Duration::from_millis(1) {
        tokio::time::sleep(Duration::from_millis(1)).await;
      }
    }
  });

  debug!(event = "cli_pty_detach_send", %attachment_id, "sending detach");
  let _ = rt.block_on(async { client::pty_detach(&sock, &attachment_id).await });
  // In non-TTY, we still emit the reset footer if forced
  let force = std::env::var("AGENCY_FORCE_TTY_RESET").ok().is_some();
  if force || std::io::stdout().is_terminal() {
    let mut out = std::io::stdout();
    let _ = term_reset::write_reset_footer(&mut out);
  }
  eprintln!("detached");
}
