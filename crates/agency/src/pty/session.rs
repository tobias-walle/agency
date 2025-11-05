//! Session lifecycle and PTY bridging.
//!
//! A `Session` owns a pseudo-terminal (PTY) that runs a shell (`sh`). It reads
//! bytes produced by the PTY, feeds them into a `vt100::Parser` to maintain a
//! virtual screen model, and optionally forwards raw bytes to a connected client
//! via a channel. Input bytes from a client are written to the PTY writer.
//!
//! This design decouples the PTY from any particular client connection: the
//! session can continue running even if no client is attached. When a client
//! attaches, we configure the output sink so the already-running PTY stream is
//! forwarded as `D2C::Output` frames. A snapshot (`vt100` screen contents) is
//! sent separately during handshake.

use crate::pty::protocol::D2COutputChannel;
use anyhow::Context;
use portable_pty::{CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{
  Arc, Mutex,
  atomic::{AtomicU64, Ordering},
};
use std::thread;
use std::time::Instant;

pub struct Session {
  master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
  writer: Arc<Mutex<Box<dyn Write + Send>>>,
  child: Box<dyn portable_pty::Child + Send>,
  parser: Arc<Mutex<vt100::Parser>>,
  bytes_in: Arc<AtomicU64>,
  pub bytes_out: Arc<AtomicU64>,
  pub start: Instant,
  output_sink: Arc<Mutex<Option<D2COutputChannel>>>,
}

impl Session {
  /// Create a new session running a shell in a PTY with the given size.
  pub fn new(rows: u16, cols: u16) -> anyhow::Result<Self> {
    let pty_size = PtySize {
      rows,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    };
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(pty_size)?;

    let cmd = CommandBuilder::new("sh");
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 10_000)));
    let bytes_in = Arc::new(AtomicU64::new(0));
    let bytes_out = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let master: Box<dyn MasterPty + Send> = pair.master;
    let writer = master.take_writer().context("failed to take PTY writer")?;
    let master = Arc::new(Mutex::new(master));
    let writer = Arc::new(Mutex::new(writer));

    let output_sink: Arc<Mutex<Option<D2COutputChannel>>> = Arc::new(Mutex::new(None));

    let sess = Self {
      master,
      writer,
      child,
      parser,
      bytes_in,
      bytes_out,
      start,
      output_sink,
    };
    sess.start_read_pump();
    Ok(sess)
  }

  /// Restart the shell with a new PTY of the provided size.
  /// Replaces `master`, `writer`, and `child`, resets the parser, and spawns
  /// a fresh read pump bound to the new PTY.
  pub fn restart_shell(&mut self, rows: u16, cols: u16) -> anyhow::Result<()> {
    let pty_size = PtySize {
      rows,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    };
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(pty_size)?;

    let cmd = CommandBuilder::new("sh");
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    let master: Box<dyn MasterPty + Send> = pair.master;
    let writer = master.take_writer().context("failed to take PTY writer")?;
    let master = Arc::new(Mutex::new(master));
    let writer = Arc::new(Mutex::new(writer));

    // Replace master, writer, and child
    self.master = master;
    self.writer = writer;
    self.child = child;

    // Reset parser to the new size
    if let Ok(mut p) = self.parser.lock() {
      *p = vt100::Parser::new(rows, cols, 10_000);
    }

    // Start a fresh PTY read pump bound to the new master
    self.start_read_pump();

    Ok(())
  }

  /// Spawn a background thread that reads raw bytes from the PTY, updates the
  /// `vt100` parser, and forwards bytes to the `output_sink` channel if set.
  fn start_read_pump(&self) {
    let master_for_read = self.master.clone();
    let parser = self.parser.clone();
    let bytes_out = self.bytes_out.clone();
    let output_sink = self.output_sink.clone();

    thread::spawn(move || {
      let mut reader = match master_for_read.lock() {
        Ok(m) => m.try_clone_reader().expect("failed to clone PTY reader"),
        Err(_) => return,
      };
      let mut buf = [0u8; 8192];
      loop {
        match reader.read(&mut buf) {
          Ok(0) => break, // PTY closed
          Ok(n) => {
            if let Ok(mut p) = parser.lock() {
              p.process(&buf[..n]);
            }
            bytes_out.fetch_add(n as u64, Ordering::Relaxed);
            // Forward to client if attached (non-blocking, lossy)
            let opt = output_sink.lock().ok().and_then(|g| g.as_ref().cloned());
            if let Some(out) = opt {
              let _ = out.try_send_bytes(&buf[..n]);
            }
          }
          Err(_) => break,
        }
      }
    });
  }

  pub fn apply_resize(&self, rows: u16, cols: u16) {
    if let Ok(m) = self.master.lock() {
      let _ = m.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
      });
    }
    if let Ok(mut p) = self.parser.lock() {
      p.screen_mut().set_size(rows, cols);
    }
  }

  pub fn write_input(&self, bytes: &[u8]) -> anyhow::Result<()> {
    self
      .bytes_in
      .fetch_add(bytes.len() as u64, Ordering::Relaxed);
    if let Ok(mut w) = self.writer.lock() {
      w.write_all(bytes)?;
      let _ = w.flush();
    }
    Ok(())
  }

  pub fn snapshot(&self) -> (Vec<u8>, (u16, u16)) {
    let p = self.parser.lock().unwrap();
    let screen = p.screen();
    let ansi = screen.contents_formatted();
    let (rows, cols) = screen.size();
    (ansi, (rows, cols))
  }

  /// Returns the current PTY size as `(rows, cols)` without generating ANSI.
  pub fn size(&self) -> (u16, u16) {
    let p = self.parser.lock().unwrap();
    let screen = p.screen();
    let (rows, cols) = screen.size();
    (rows, cols)
  }

  pub fn stats_lite(&self) -> crate::pty::protocol::SessionStatsLite {
    let elapsed = self.start.elapsed();
    crate::pty::protocol::SessionStatsLite {
      bytes_in: self.bytes_in.load(std::sync::atomic::Ordering::Relaxed),
      bytes_out: self.bytes_out.load(std::sync::atomic::Ordering::Relaxed),
      elapsed_ms: elapsed.as_millis() as u64,
    }
  }

  #[allow(dead_code)]
  pub fn wait_child(&mut self) -> anyhow::Result<ExitStatus> {
    let status = self.child.wait()?;
    Ok(status)
  }

  pub fn try_wait_child(&mut self) -> Option<ExitStatus> {
    match self.child.try_wait() {
      Ok(Some(status)) => Some(status),
      Ok(None) => None,
      Err(_) => None,
    }
  }

  /// Sets or clears the output sink. The provided channel is cloned under
  /// a mutex and used by the read pump outside of any locks to avoid
  /// lock-held blocking.
  pub fn set_output_sink(&self, sink: Option<D2COutputChannel>) {
    if let Ok(mut guard) = self.output_sink.lock() {
      *guard = sink;
    }
  }
}
