use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, unbounded};
use serde::{Deserialize, Serialize};
use serde::{Serialize as SerdeSerialize, de::DeserializeOwned};
use std::io::{Read, Write};

/// Identifies a project by its canonical repository root directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectKey {
  pub repo_root: String,
}

/// Identifies a task within a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskMeta {
  pub id: u32,
  pub slug: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionStatsLite {
  /// Total number of bytes read from client and written to PTY.
  pub bytes_in: u64,
  /// Total number of bytes produced by PTY and forwarded to client.
  pub bytes_out: u64,
  /// Elapsed session time in milliseconds since start.
  pub elapsed_ms: u64,
}

/// Command description used for launching sessions (serde-friendly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireCommand {
  pub program: String,
  pub args: Vec<String>,
  pub cwd: String,
  pub env: Vec<(String, String)>,
}

/// Open-session metadata sent by the client to the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOpenMeta {
  pub project: ProjectKey,
  pub task: TaskMeta,
  pub worktree_dir: String,
  pub cmd: WireCommand,
}

/// Control messages sent from the client to the daemon.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum C2DControl {
  /// Create a new session for a task and attach as a client.
  OpenSession {
    meta: SessionOpenMeta,
    rows: u16,
    cols: u16,
  },
  /// Join an existing session by id.
  JoinSession {
    session_id: u64,
    rows: u16,
    cols: u16,
  },
  /// Resize notification with new terminal rows/cols.
  Resize { rows: u16, cols: u16 },
  /// Detach request to end this client attachment.
  Detach,
  /// Request restart of the given session's shell.
  RestartSession { session_id: u64 },
  /// Stop and remove the given session.
  StopSession { session_id: u64 },
  /// Stop all sessions for a given task.
  StopTask {
    project: ProjectKey,
    task_id: u32,
    slug: String,
  },
  /// List sessions with optional project filter.
  ListSessions { project: Option<ProjectKey> },
  /// Subscribe to daemon events for a project (long-lived connection).
  SubscribeEvents { project: ProjectKey },
  /// Notify daemon that tasks changed for a project.
  NotifyTasksChanged { project: ProjectKey },
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

/// Summary of a session for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
  pub session_id: u64,
  pub project: ProjectKey,
  pub task: TaskMeta,
  pub cwd: String,
  pub status: String,
  pub clients: u32,
  pub created_at_ms: u64,
  pub stats: SessionStatsLite,
}

impl Default for SessionInfo {
  fn default() -> Self {
    Self {
      session_id: 0,
      project: ProjectKey {
        repo_root: String::new(),
      },
      task: TaskMeta {
        id: 0,
        slug: String::new(),
      },
      cwd: String::new(),
      status: String::new(),
      clients: 0,
      created_at_ms: 0,
      stats: SessionStatsLite {
        bytes_in: 0,
        bytes_out: 0,
        elapsed_ms: 0,
      },
    }
  }
}

/// Control messages sent from the daemon to the client.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum D2CControl {
  /// Welcome message carrying session id, initial size and ANSI snapshot.
  Welcome {
    session_id: u64,
    rows: u16,
    cols: u16,
    ansi: Vec<u8>,
  },
  /// Notification that the shell process exited, with optional code/signal and stats.
  Exited {
    code: Option<i32>,
    signal: Option<i32>,
    stats: SessionStatsLite,
  },
  /// List of sessions returned for a query.
  Sessions { entries: Vec<SessionInfo> },
  /// Event: Sessions changed (delta implicit by client refresh).
  SessionsChanged { entries: Vec<SessionInfo> },
  /// Event: Tasks changed for a project.
  TasksChanged { project: ProjectKey },
  /// Generic acknowledgement with a count (e.g., number of sessions stopped).
  Ack { stopped: usize },
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
  let len = u32::try_from(data.len()).context("payload too large for frame header (u32)")?;
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
  /// Sends any control message (internal helper for broadcast paths).
  pub fn send(&self, msg: D2CControl) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self.tx.send(msg).map_err(Box::new)
  }
  /// Sends a `Welcome` control message with session id and initial snapshot.
  pub fn send_welcome(
    &self,
    session_id: u64,
    rows: u16,
    cols: u16,
    ansi: Vec<u8>,
  ) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self
      .tx
      .send(D2CControl::Welcome {
        session_id,
        rows,
        cols,
        ansi,
      })
      .map_err(Box::new)
  }

  /// Sends an `Exited` control message with stats.
  pub fn send_exited(
    &self,
    code: Option<i32>,
    signal: Option<i32>,
    stats: SessionStatsLite,
  ) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self
      .tx
      .send(D2CControl::Exited {
        code,
        signal,
        stats,
      })
      .map_err(Box::new)
  }

  /// Sends a `Sessions` control message.
  pub fn send_sessions(
    &self,
    entries: Vec<SessionInfo>,
  ) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self
      .tx
      .send(D2CControl::Sessions { entries })
      .map_err(Box::new)
  }

  /// Sends a `Goodbye` control message.
  pub fn send_goodbye(&self) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self.tx.send(D2CControl::Goodbye).map_err(Box::new)
  }

  /// Sends a `Pong` response.
  pub fn send_pong(&self, nonce: u64) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self.tx.send(D2CControl::Pong { nonce }).map_err(Box::new)
  }

  /// Sends an `Error` control message.
  pub fn send_error(
    &self,
    message: String,
  ) -> Result<(), Box<crossbeam_channel::SendError<D2CControl>>> {
    self
      .tx
      .send(D2CControl::Error { message })
      .map_err(Box::new)
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
  #[must_use]
  pub fn try_send_bytes(&self, bytes: &[u8]) -> bool {
    match self.tx.try_send(bytes.to_vec()) {
      Ok(()) => true,
      Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
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
  /// Sends an `OpenSession` request with metadata and initial size.
  pub fn send_open_session(
    &self,
    meta: SessionOpenMeta,
    rows: u16,
    cols: u16,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::OpenSession { meta, rows, cols })
      .map_err(Box::new)
  }

  /// Sends a `JoinSession` request for an existing session id.
  pub fn send_join_session(
    &self,
    session_id: u64,
    rows: u16,
    cols: u16,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::JoinSession {
        session_id,
        rows,
        cols,
      })
      .map_err(Box::new)
  }

  /// Sends a `Detach` request.
  pub fn send_detach(&self) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self.tx.send(C2DControl::Detach).map_err(Box::new)
  }

  /// Sends a `Resize` notification.
  pub fn send_resize(
    &self,
    rows: u16,
    cols: u16,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::Resize { rows, cols })
      .map_err(Box::new)
  }

  /// Sends a `RestartSession` request.
  pub fn send_restart_session(
    &self,
    session_id: u64,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::RestartSession { session_id })
      .map_err(Box::new)
  }

  /// Sends a `StopSession` request.
  pub fn send_stop_session(
    &self,
    session_id: u64,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::StopSession { session_id })
      .map_err(Box::new)
  }

  /// Sends a `StopTask` request.
  pub fn send_stop_task(
    &self,
    project: ProjectKey,
    task_id: u32,
    slug: String,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::StopTask {
        project,
        task_id,
        slug,
      })
      .map_err(Box::new)
  }

  /// Sends a `ListSessions` request.
  pub fn send_list_sessions(
    &self,
    project: Option<ProjectKey>,
  ) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self
      .tx
      .send(C2DControl::ListSessions { project })
      .map_err(Box::new)
  }

  /// Sends a `Ping` with the given nonce.
  #[allow(dead_code)]
  pub fn send_ping(&self, nonce: u64) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self.tx.send(C2DControl::Ping { nonce }).map_err(Box::new)
  }

  /// Sends a `Shutdown` control.
  pub fn send_shutdown(&self) -> Result<(), Box<crossbeam_channel::SendError<C2DControl>>> {
    self.tx.send(C2DControl::Shutdown).map_err(Box::new)
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
  pub fn send_input(&self, bytes: &[u8]) -> Result<(), Box<crossbeam_channel::SendError<Vec<u8>>>> {
    self.tx.send(bytes.to_vec()).map_err(Box::new)
  }
}

/// Creates an unbounded daemon control channel suitable for low-volume reliable frames.
/// Returns the sender wrapper and a receiver for the writer thread.
#[must_use]
pub fn make_d2c_control_channel() -> (D2CControlChannel, Receiver<D2CControl>) {
  let (tx, rx) = unbounded::<D2CControl>();
  (D2CControlChannel { tx }, rx)
}

/// Creates a bounded lossy output channel with the given capacity in chunks.
/// Returns the sender wrapper and a receiver for the writer thread.
#[must_use]
pub fn make_output_channel(capacity: usize) -> (D2COutputChannel, Receiver<Vec<u8>>) {
  let (tx, rx) = bounded::<Vec<u8>>(capacity);
  (D2COutputChannel { tx }, rx)
}

/// Creates an unbounded client control channel.
#[must_use]
pub fn make_c2d_control_channel() -> (C2DControlChannel, Receiver<C2DControl>) {
  let (tx, rx) = unbounded::<C2DControl>();
  (C2DControlChannel { tx }, rx)
}

/// Creates an unbounded client input channel.
#[must_use]
pub fn make_c2d_input_channel() -> (C2DInputChannel, Receiver<Vec<u8>>) {
  let (tx, rx) = unbounded::<Vec<u8>>();
  (C2DInputChannel { tx }, rx)
}
