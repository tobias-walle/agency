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

use crate::log_warn;
use crate::pty::idle::{IdleState, IdleTracker};
use crate::pty::protocol::D2COutputChannel;
use crate::utils::command::Command;
use anyhow::Context;
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::sync::{
  Arc,
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
  idle: Arc<Mutex<IdleTracker>>,
  output_sinks: Arc<Mutex<Vec<D2COutputChannel>>>,
  cmd: Command,
}

impl Session {
  /// Create a new session running a shell in a PTY with the given size.
  pub fn new(rows: u16, cols: u16, cmd: Command) -> anyhow::Result<Self> {
    let pty_size = PtySize {
      rows,
      cols,
      pixel_width: 0,
      pixel_height: 0,
    };
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(pty_size)?;

    let builder = Self::build_pty_command_for(&cmd);
    let child = pair.slave.spawn_command(builder)?;
    drop(pair.slave);

    let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 10_000)));
    let bytes_in = Arc::new(AtomicU64::new(0));
    let bytes_out = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    let idle = Arc::new(Mutex::new(IdleTracker::new(start)));

    let master: Box<dyn MasterPty + Send> = pair.master;
    let writer = master.take_writer().context("failed to take PTY writer")?;
    let master = Arc::new(Mutex::new(master));
    let writer = Arc::new(Mutex::new(writer));

    let output_sinks: Arc<Mutex<Vec<D2COutputChannel>>> = Arc::new(Mutex::new(Vec::new()));

    let sess = Self {
      master,
      writer,
      child,
      parser,
      bytes_in,
      bytes_out,
      start,
      idle,
      output_sinks,
      cmd,
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

    let builder = Self::build_pty_command_for(&self.cmd);
    let child = pair.slave.spawn_command(builder)?;
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
    {
      let mut p = self.parser.lock();
      *p = vt100::Parser::new(rows, cols, 10_000);
    }
    {
      let now = Instant::now();
      let mut idle = self.idle.lock();
      *idle = IdleTracker::new(now);
      self.start = now;
    }

    // Start a fresh PTY read pump bound to the new master
    self.start_read_pump();

    Ok(())
  }

  /// Build a `portable_pty::CommandBuilder` from our Command
  fn build_pty_command_for(cmd: &Command) -> CommandBuilder {
    let mut builder = CommandBuilder::new(&cmd.program);
    for a in &cmd.args {
      builder.arg(a);
    }
    // Ensure the child process resolves relative paths based on the original cwd
    // captured when the Command was created.
    builder.cwd(&cmd.cwd);
    // Provide environment variables to the child process
    for (k, v) in &cmd.env {
      builder.env(k, v);
    }
    builder
  }

  /// Spawn a background thread that reads raw bytes from the PTY, updates the
  /// `vt100` parser, and forwards bytes to the `output_sink` channel if set.
  fn start_read_pump(&self) {
    let master_for_read = self.master.clone();
    let parser = self.parser.clone();
    let bytes_out = self.bytes_out.clone();
    let sinks = self.output_sinks.clone();
    let idle = self.idle.clone();
    let writer = self.writer.clone();

    thread::spawn(move || {
      let mut reader = master_for_read
        .lock()
        .try_clone_reader()
        .expect("failed to clone PTY reader");
      let mut buf = [0u8; 8192];
      loop {
        match reader.read(&mut buf) {
          Ok(0) | Err(_) => break, // PTY closed or error
          Ok(n) => {
            let chunk = &buf[..n];
            {
              let mut p = parser.lock();
              p.process(chunk);
            }
            {
              let mut tracker = idle.lock();
              let now = Instant::now();
              tracker.record_output(now, chunk);
            }
            bytes_out.fetch_add(n as u64, Ordering::Relaxed);
            // Forward to all attached clients (non-blocking, lossy)
            let guard = sinks.lock();
            for out in guard.iter() {
              let _ = out.try_send_bytes(chunk);
            }
          }
        }
      }
    });
  }

  pub fn apply_resize(&self, rows: u16, cols: u16) {
    {
      let m = self.master.lock();
      let _ = m.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
      });
    }
    {
      let mut p = self.parser.lock();
      p.screen_mut().set_size(rows, cols);
    }
  }

  pub fn write_input(&self, bytes: &[u8]) -> anyhow::Result<()> {
    self
      .bytes_in
      .fetch_add(bytes.len() as u64, Ordering::Relaxed);
    {
      let mut tracker = self.idle.lock();
      tracker.record_input(Instant::now());
    }
    let mut w = self.writer.lock();
    w.write_all(bytes)?;
    let _ = w.flush();
    Ok(())
  }

  #[must_use]
  #[allow(clippy::missing_panics_doc)]
  pub fn snapshot(&self) -> (Vec<u8>, (u16, u16)) {
    let p = self.parser.lock();
    let screen = p.screen();
    let ansi = screen.contents_formatted();
    let (rows, cols) = screen.size();
    (ansi, (rows, cols))
  }

  /// Returns the current PTY size as `(rows, cols)` without generating ANSI.
  #[must_use]
  #[allow(clippy::missing_panics_doc)]
  pub fn size(&self) -> (u16, u16) {
    let p = self.parser.lock();
    let screen = p.screen();
    let (rows, cols) = screen.size();
    (rows, cols)
  }

  #[must_use]
  pub fn stats_lite(&self) -> crate::pty::protocol::SessionStatsLite {
    let elapsed = self.start.elapsed();
    crate::pty::protocol::SessionStatsLite {
      bytes_in: self.bytes_in.load(std::sync::atomic::Ordering::Relaxed),
      bytes_out: self.bytes_out.load(std::sync::atomic::Ordering::Relaxed),
      elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
    }
  }

  #[must_use]
  pub fn poll_idle(&self, now: Instant) -> (IdleState, bool) {
    let mut tracker = self.idle.lock();
    tracker.poll(now)
  }

  #[allow(dead_code)]
  pub fn wait_child(&mut self) -> anyhow::Result<ExitStatus> {
    let status = self.child.wait()?;
    Ok(status)
  }

  pub fn try_wait_child(&mut self) -> Option<ExitStatus> {
    match self.child.try_wait() {
      Ok(Some(status)) => Some(status),
      _ => None,
    }
  }

  /// Add a new output sink for a client attachment.
  pub fn add_output_sink(&mut self, sink: D2COutputChannel) {
    let mut guard = self.output_sinks.lock();
    guard.push(sink);
  }

  /// Remove an output sink for a client detachment.
  pub fn remove_output_sink(&mut self, sink: &D2COutputChannel) {
    let mut guard = self.output_sinks.lock();
    guard.retain(|s| !std::ptr::eq(s, sink));
  }

  /// Clear all output sinks (used when last client detaches or on restart).
  pub fn clear_all_sinks(&mut self) {
    let mut guard = self.output_sinks.lock();
    guard.clear();
  }

  /// Attempt to stop the child process.
  pub fn stop(&mut self) -> anyhow::Result<()> {
    let _ = self.child.kill();
    Ok(())
  }
}
