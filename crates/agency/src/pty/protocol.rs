use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};
use serde::{Deserialize, Serialize};
use serde::{Serialize as SerdeSerialize, de::DeserializeOwned};
use std::io::{Read, Write};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionStatsLite {
  /// Total number of bytes read from client and written to PTY.
  pub bytes_in: u64,
  /// Total number of bytes produced by PTY and forwarded to client.
  pub bytes_out: u64,
  /// Elapsed session time in milliseconds since start.
  pub elapsed_ms: u64,
}

/// Control messages sent from the client to the daemon.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum C2DControl {
  /// Attach request with initial terminal size and optional client name.
  Attach {
    rows: u16,
    cols: u16,
    client_name: Option<String>,
    /// Optional task payload carried by the client for the daemon to run.
    /// When present, the daemon may use it to configure the agent process.
    task: Option<String>,
  },
  /// Resize notification with new terminal rows/cols.
  Resize { rows: u16, cols: u16 },
  /// Detach request to end the attachment.
  Detach,
  /// Ping for liveness checks carrying a nonce echoed by the daemon.
  Ping { nonce: u64 },
  /// Request the daemon to shutdown gracefully.
  Shutdown,
}

/// Top-level client-to-daemon protocol frames.
///
/// Distinguishes raw input bytes from higher-level control messages.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum C2D {
  /// Control message wrapper.
  Control(C2DControl),
  /// Raw stdin bytes to be written to the PTY.
  Input { bytes: Vec<u8> },
}

/// Control messages sent from the daemon to the client.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum D2CControl {
  /// Initial hello carrying the current PTY size.
  Hello { pty_rows: u16, pty_cols: u16 },
  /// ANSI snapshot of the current screen and its size.
  Snapshot { ansi: Vec<u8>, rows: u16, cols: u16 },
  /// Notification that the shell process exited, with optional code/signal and stats.
  Exited {
    code: Option<i32>,
    signal: Option<i32>,
    stats: SessionStatsLite,
  },
  /// Goodbye indicates the daemon acknowledges the detach and will close the connection.
  Goodbye,
  /// Error message explaining a protocol or lifecycle issue.
  Error { message: String },
  /// Pong response echoing the provided nonce.
  Pong { nonce: u64 },
}

/// Top-level daemon-to-client protocol frames.
///
/// Distinguishes raw PTY output bytes from higher-level control messages.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum D2C {
  /// Control message wrapper.
  Control(D2CControl),
  /// Raw PTY output bytes forwarded to the client.
  Output { bytes: Vec<u8> },
}

/// Number of bytes in the frame header (little-endian `u32`).
pub const FRAME_HEADER_LEN: usize = 4;

/// Writes one framed payload to the given writer.
pub fn write_frame<W: Write, T: SerdeSerialize>(mut writer: W, payload: &T) -> Result<()> {
  let data = bincode::serde::encode_to_vec(payload, bincode::config::standard())
    .context("encode payload with bincode")?;
  let len = data.len() as u32;
  let hdr = len.to_le_bytes();
  writer.write_all(&hdr).context("write frame header (len)")?;
  writer
    .write_all(&data)
    .context("write frame payload bytes")?;
  Ok(())
}

/// Reads one framed payload from the given reader.
pub fn read_frame<R: Read, T: DeserializeOwned>(mut reader: R) -> Result<T> {
  let mut hdr = [0u8; FRAME_HEADER_LEN];
  reader
    .read_exact(&mut hdr)
    .context("read frame header (len)")?;
  let len = u32::from_le_bytes(hdr) as usize;
  let mut buf = vec![0u8; len];
  reader
    .read_exact(&mut buf)
    .context("read frame payload bytes")?;
  let (msg, _): (T, usize) =
    bincode::serde::decode_from_slice::<T, _>(&buf, bincode::config::standard())
      .context("decode payload with bincode")?;
  Ok(msg)
}

/// Reliable daemon-to-client control channel carrying only `D2CControl` frames.
#[derive(Clone)]
pub struct D2CControlChannel {
  /// Underlying sender for reliable control frames to the client.
  tx: Sender<D2CControl>,
}

impl D2CControlChannel {
  /// Sends a `Hello` control message with PTY size.
  pub fn send_hello(
    &self,
    rows: u16,
    cols: u16,
  ) -> Result<(), crossbeam_channel::SendError<D2CControl>> {
    self.tx.send(D2CControl::Hello {
      pty_rows: rows,
      pty_cols: cols,
    })
  }

  /// Sends a `Snapshot` control message with ANSI and size.
  pub fn send_snapshot(
    &self,
    ansi: Vec<u8>,
    rows: u16,
    cols: u16,
  ) -> Result<(), crossbeam_channel::SendError<D2CControl>> {
    self.tx.send(D2CControl::Snapshot { ansi, rows, cols })
  }

  /// Sends an `Exited` control message with stats.
  pub fn send_exited(
    &self,
    code: Option<i32>,
    signal: Option<i32>,
    stats: SessionStatsLite,
  ) -> Result<(), crossbeam_channel::SendError<D2CControl>> {
    self.tx.send(D2CControl::Exited {
      code,
      signal,
      stats,
    })
  }

  /// Sends a `Goodbye` control message.
  pub fn send_goodbye(&self) -> Result<(), crossbeam_channel::SendError<D2CControl>> {
    self.tx.send(D2CControl::Goodbye)
  }

  /// Sends a `Pong` response.
  pub fn send_pong(&self, nonce: u64) -> Result<(), crossbeam_channel::SendError<D2CControl>> {
    self.tx.send(D2CControl::Pong { nonce })
  }

  /// Sends an `Error` control message.
  pub fn send_error(
    &self,
    message: String,
  ) -> Result<(), crossbeam_channel::SendError<D2CControl>> {
    self.tx.send(D2CControl::Error { message })
  }
}

/// Lossy daemon-to-client output channel carrying raw PTY byte chunks.
#[derive(Clone)]
pub struct D2COutputChannel {
  /// Bounded lossy channel carrying raw PTY output byte chunks.
  tx: Sender<Vec<u8>>,
}

impl D2COutputChannel {
  /// Attempts to enqueue output bytes without blocking.
  pub fn try_send_bytes(&self, bytes: &[u8]) -> bool {
    match self.tx.try_send(bytes.to_vec()) {
      Ok(()) => true,
      Err(TrySendError::Full(_)) => false,
      Err(TrySendError::Disconnected(_)) => false,
    }
  }
}

/// Reliable client-to-daemon control channel carrying only `C2DControl` frames.
#[derive(Clone)]
pub struct C2DControlChannel {
  /// Unbounded client-side control channel for C2D control frames.
  tx: Sender<C2DControl>,
}

impl C2DControlChannel {
  /// Sends an `Attach` request with initial size and optional client name.
  pub fn send_attach(
    &self,
    rows: u16,
    cols: u16,
    client_name: Option<String>,
    task: Option<String>,
  ) -> Result<(), crossbeam_channel::SendError<C2DControl>> {
    self.tx.send(C2DControl::Attach {
      rows,
      cols,
      client_name,
      task,
    })
  }

  /// Sends a `Detach` request.
  pub fn send_detach(&self) -> Result<(), crossbeam_channel::SendError<C2DControl>> {
    self.tx.send(C2DControl::Detach)
  }

  /// Sends a `Resize` notification.
  pub fn send_resize(
    &self,
    rows: u16,
    cols: u16,
  ) -> Result<(), crossbeam_channel::SendError<C2DControl>> {
    self.tx.send(C2DControl::Resize { rows, cols })
  }

  /// Sends a `Ping` with the given nonce.
  #[allow(dead_code)]
  pub fn send_ping(&self, nonce: u64) -> Result<(), crossbeam_channel::SendError<C2DControl>> {
    self.tx.send(C2DControl::Ping { nonce })
  }

  /// Sends a `Shutdown` control.
  pub fn send_shutdown(&self) -> Result<(), crossbeam_channel::SendError<C2DControl>> {
    self.tx.send(C2DControl::Shutdown)
  }
}

/// Unbounded client-side input channel carrying raw stdin bytes.
#[derive(Clone)]
pub struct C2DInputChannel {
  /// Unbounded client-side input channel carrying raw stdin bytes.
  tx: Sender<Vec<u8>>,
}

impl C2DInputChannel {
  /// Enqueues input bytes for the writer thread. Unbounded; does not block.
  pub fn send_input(&self, bytes: &[u8]) -> Result<(), crossbeam_channel::SendError<Vec<u8>>> {
    self.tx.send(bytes.to_vec())
  }
}

/// Creates an unbounded daemon control channel suitable for low-volume reliable frames.
/// Returns the sender wrapper and a receiver for the writer thread.
pub fn make_d2c_control_channel() -> (D2CControlChannel, Receiver<D2CControl>) {
  let (tx, rx) = unbounded::<D2CControl>();
  (D2CControlChannel { tx }, rx)
}

/// Creates a bounded lossy output channel with the given capacity in chunks.
/// Returns the sender wrapper and a receiver for the writer thread.
pub fn make_output_channel(capacity: usize) -> (D2COutputChannel, Receiver<Vec<u8>>) {
  let (tx, rx) = bounded::<Vec<u8>>(capacity);
  (D2COutputChannel { tx }, rx)
}

/// Creates an unbounded client control channel.
pub fn make_c2d_control_channel() -> (C2DControlChannel, Receiver<C2DControl>) {
  let (tx, rx) = unbounded::<C2DControl>();
  (C2DControlChannel { tx }, rx)
}

/// Creates an unbounded client input channel.
pub fn make_c2d_input_channel() -> (C2DInputChannel, Receiver<Vec<u8>>) {
  let (tx, rx) = unbounded::<Vec<u8>>();
  (C2DInputChannel { tx }, rx)
}
