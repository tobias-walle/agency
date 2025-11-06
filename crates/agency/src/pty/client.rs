use crate::pty::protocol::{
  C2D, C2DControl, C2DControlChannel, C2DInputChannel, D2C, D2CControl, SessionOpenMeta,
  make_c2d_control_channel, make_c2d_input_channel, read_frame, write_frame,
};
use anyhow::{Context, Result, anyhow};
use crossbeam_channel::Receiver;
use crossterm::terminal;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod tty;
use tty::{RawModeGuard, RawModePauseGuard};

/// Attaches to the daemon and delegates lifecycle orchestration to `Client`.
///
/// This public entrypoint remains stable for CLI and tests.
/// It sets raw mode and timed reads, connects to the daemon socket, constructs the
/// `Client` orchestrator, and runs it with a single stream (split internally).
pub fn run_attach(
  socket_path: &Path,
  open: SessionOpenMeta,
  join_session_id: Option<u64>,
) -> Result<()> {
  let _raw = RawModeGuard::enable()?;

  let stream = UnixStream::connect(socket_path).context("failed to connect to daemon socket")?;

  let mut client = Client::new(open, join_session_id);
  client.run(stream)
}

/// Client orchestrator responsible for managing the attached-only lifecycle.
///
/// Invariants:
/// - The writer thread owns the socket writer and is the only place that calls `write_frame`.
/// - No external locks are held while sending frames; channels are used to communicate with the writer.
/// - Control priority: control frames are drained eagerly and selected before input frames.
/// - Attached-only lifecycle: the client exits on `Goodbye` or disconnect; no idle or multi-phase states.
struct Client {
  /// Unbounded client-side control channel sender used to request `Attach`, `Resize`, `Detach`, `Ping`.
  control_tx: Option<C2DControlChannel>,
  /// Unbounded client-side input channel sender carrying raw stdin bytes to the writer thread.
  input_tx: Option<C2DInputChannel>,
  /// Receiver for client control frames consumed by the writer thread.
  control_rx: Receiver<C2DControl>,
  /// Receiver for stdin byte chunks consumed by the writer thread.
  input_rx: Receiver<Vec<u8>>,
  /// Shared flag to coordinate shutdown across threads.
  running: Arc<AtomicBool>,
  /// Open-session metadata to send when creating a new session.
  open_meta: SessionOpenMeta,
  /// Optional session id to join instead of creating one.
  join_session: Option<u64>,
  /// Remembered session id from Welcome.
  session_id: Option<u64>,
}

impl Client {
  /// Constructs a new attached-only client orchestrator and its internal channels.
  fn new(open: SessionOpenMeta, join_session: Option<u64>) -> Self {
    let (control_tx, control_rx) = make_c2d_control_channel();
    let (input_tx, input_rx) = make_c2d_input_channel();
    Self {
      control_tx: Some(control_tx),
      input_tx: Some(input_tx),
      control_rx,
      input_rx,
      running: Arc::new(AtomicBool::new(true)),
      open_meta: open,
      join_session,
      session_id: None,
    }
  }

  /// Performs the initial handshake:
  /// - Sends `OpenSession` or `JoinSession` via the control channel.
  /// - Reads `Welcome` from the daemon and writes the ANSI snapshot to stdout.
  fn handshake(&mut self, stream_reader: &mut UnixStream, rows: u16, cols: u16) -> Result<()> {
    if let Some(ref tx) = self.control_tx {
      if let Some(session_id) = self.join_session {
        let _ = tx.send_join_session(session_id, rows, cols);
      } else {
        let _ = tx.send_open_session(self.open_meta.clone(), rows, cols);
      }
    }

    let msg: D2C = read_frame(&mut *stream_reader)?;
    match msg {
      D2C::Control(D2CControl::Welcome {
        session_id, ansi, ..
      }) => {
        self.session_id = Some(session_id);
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(&ansi);
        let _ = stdout.flush();
        Ok(())
      }
      D2C::Control(D2CControl::Error { message }) => {
        eprintln!("Daemon error: {}", message);
        Err(anyhow!(message))
      }
      other => {
        let _ = other;
        eprintln!("Protocol error: expected Welcome");
        Err(anyhow!("protocol: expected Welcome"))
      }
    }
  }

  /// Spawns the client writer thread with control priority and owned stream.
  /// The thread drains control frames eagerly and interleaves input frames.
  fn spawn_writer_thread(&self, stream_writer: UnixStream) -> JoinHandle<()> {
    let control_rx = self.control_rx.clone();
    let input_rx = self.input_rx.clone();
    thread::Builder::new()
            .name("client-writer".to_string())
            .spawn(move || {
                let mut writer = stream_writer;
                let _ = writer.set_write_timeout(Some(Duration::from_secs(1)));
                loop {
                    // Drain control frames eagerly
                    while let Ok(control_msg) = control_rx.try_recv() {
                        if let Err(err) = write_frame(&mut writer, &C2D::Control(control_msg)) {
                            let _ = err; // silent exit on writer failure
                            return;
                        }
                    }
                    crossbeam_channel::select! {
                        recv(control_rx) -> msg => {
                            match msg {
                                Ok(control_msg) => {
                                    if write_frame(&mut writer, &C2D::Control(control_msg)).is_err() { break; }
                                }
                                Err(_) => { break; }
                            }
                        }
                        recv(input_rx) -> msg => {
                            match msg {
                                Ok(bytes) => {
                                    if write_frame(&mut writer, &C2D::Input { bytes }).is_err() { break; }
                                }
                                Err(_) => { break; }
                            }
                        }
                    }
                }
            })
            .expect("failed to spawn client writer thread")
  }

  /// Spawns the stdin reader thread that forwards bytes via the input channel.
  /// Detects Ctrl-Q (0x11) to send `Detach` and initiate shutdown.
  fn spawn_input_thread(&self) -> JoinHandle<()> {
    let send_input = self.input_tx.as_ref().unwrap().clone();
    let send_control = self.control_tx.as_ref().unwrap().clone();
    let running_flag = self.running.clone();
    thread::spawn(move || {
      let mut stdin = std::io::stdin().lock();
      let mut buffer = [0u8; 8192];
      while running_flag.load(Ordering::Relaxed) {
        match stdin.read(&mut buffer) {
          Ok(0) => continue, // timeout tick
          Ok(count) => {
            if let Some(ctrl_pos) = buffer[..count].iter().position(|&b| b == 0x11) {
              if ctrl_pos > 0 {
                let _ = send_input.send_input(&buffer[..ctrl_pos]);
              }
              let _ = send_control.send_detach();
              running_flag.store(false, Ordering::Relaxed);
              break;
            } else {
              let _ = send_input.send_input(&buffer[..count]);
            }
          }
          Err(_) => break,
        }
      }
    })
  }

  /// Spawns the resize watcher thread that sends `Resize` frames on size changes.
  fn spawn_resize_thread(&self, initial_cols: u16, initial_rows: u16) -> JoinHandle<()> {
    let send_control = self.control_tx.as_ref().unwrap().clone();
    let running_flag = self.running.clone();
    thread::spawn(move || {
      let mut last = (initial_cols, initial_rows);
      while running_flag.load(Ordering::Relaxed) {
        if let Ok((cols_now, rows_now)) = terminal::size()
          && (cols_now, rows_now) != last
        {
          last = (cols_now, rows_now);
          let _ = send_control.send_resize(rows_now, cols_now);
        }
        thread::sleep(Duration::from_millis(150));
      }
    })
  }

  /// Spawns the output reader thread that reads `D2C` frames and prints to stdout.
  /// Logs session stats on `Exited`, and breaks on `Goodbye` or errors.
  fn spawn_output_thread(&self, mut stream_reader: UnixStream) -> JoinHandle<()> {
    let running_flag = self.running.clone();
    let control_for_restart = self.control_tx.clone();
    let session_id_for_restart = self.session_id;
    thread::spawn(move || {
      let mut stdout = std::io::stdout().lock();
      let mut printed_exited = bool::default();
      while let Ok(message) = read_frame(&mut stream_reader) {
        match message {
          D2C::Output { bytes } => {
            if stdout.write_all(&bytes).is_err() {
              break;
            }
            let _ = stdout.flush();
          }
          D2C::Control(cm) => match cm {
            D2CControl::Error { message } => {
              eprintln!("Daemon error: {}", message);
              break;
            }
            D2CControl::Exited { stats, .. } => {
              if !printed_exited {
                let _pause = RawModePauseGuard::pause();
                eprintln!(
                  "\nAgent exited. Stats: in={} out={} elapsed={}ms",
                  stats.bytes_in, stats.bytes_out, stats.elapsed_ms
                );
                eprintln!("Press Enter to restart the session...");
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                if let (Some(tx), Some(sid)) = (control_for_restart.clone(), session_id_for_restart)
                {
                  let _ = tx.send_restart_session(sid);
                }
                printed_exited = true;
              }
            }
            D2CControl::Goodbye => break,
            D2CControl::Welcome { .. } | D2CControl::Sessions { .. } | D2CControl::Pong { .. } => {}
          },
        }
      }
      running_flag.store(false, Ordering::Relaxed);
    })
  }

  /// Runs the orchestrator:
  /// - Splits stream internally, computes terminal size, spawns writer, handshake,
  ///   spawns input/resize/output threads, waits for output, and orchestrates shutdown.
  fn run(&mut self, mut stream_reader: UnixStream) -> Result<()> {
    // Split into dedicated reader and writer
    let stream_writer = stream_reader.try_clone()?;

    // Query terminal size (cols, rows) and pass rows/cols to Attach
    let (cols, rows) = terminal::size().unwrap_or((80, 24));

    let writer_handle = self.spawn_writer_thread(stream_writer);

    // Handshake (Attach -> Hello + Snapshot)
    self.handshake(&mut stream_reader, rows, cols)?;

    // Input and resize threads
    let input_handle = self.spawn_input_thread();
    let resize_handle = self.spawn_resize_thread(cols, rows);

    // Output thread uses owned reader (move it)
    let output_handle = self.spawn_output_thread(stream_reader);

    // Wait for output to finish (on Goodbye or disconnect)
    let _ = output_handle.join();
    self.running.store(false, Ordering::Relaxed);

    // Best-effort detach and close channels so writer exits
    if let Some(ref tx) = self.control_tx {
      let _ = tx.send_detach();
    }
    // Drop channel senders to signal receivers
    let _ = self.control_tx.take();
    let _ = self.input_tx.take();

    // Join input and writer threads
    let _ = input_handle.join();
    let _ = writer_handle.join();
    let _ = resize_handle.join();

    Ok(())
  }
}
