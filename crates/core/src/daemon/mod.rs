use std::path::{Path, PathBuf};
use std::{fs, io};

use jsonrpsee::server::RpcModule;
use tokio::task::JoinHandle;

pub mod api;
mod resume;
mod server;
mod task_index;

use resume::resume_running_tasks_if_configured;
use server::{shutdown_channel, start as start_server};

/// Handle to the running daemon server.
pub struct DaemonHandle {
  task: JoinHandle<()>,
  socket_path: PathBuf,
  // Keep the server handle alive to prevent immediate shutdown
  _server_handle: jsonrpsee::server::ServerHandle,
}

impl DaemonHandle {
  /// Stop the daemon task and remove the socket file if it exists.
  pub fn stop(self) {
    self.task.abort();
    let _ = fs::remove_file(&self.socket_path);
  }

  /// Await the daemon task to finish (e.g., after shutdown).
  pub async fn wait(self) {
    let _ = self.task.await;
  }

  /// Get the socket path the daemon is bound to.
  pub fn socket_path(&self) -> &Path {
    &self.socket_path
  }
}

/// Start a JSON-RPC server over a Unix domain socket using jsonrpsee.
/// Preserves the `start()` API and orchestrates server + API registration.
pub async fn start(socket_path: &Path) -> io::Result<DaemonHandle> {
  let sock = socket_path.to_path_buf();
  let mut module = RpcModule::new(sock.clone());

  // Prepare shutdown coordination and register APIs
  let (shutdown_tx, shutdown_rx) = shutdown_channel();
  api::daemon::register(&mut module, shutdown_tx.clone());
  api::tasks::register(&mut module);
  api::pty::register(&mut module);

  // Before serving, resume running tasks for configured resume root
  resume_running_tasks_if_configured();

  let (task, server_handle) = start_server(socket_path, module, shutdown_rx)?;

  Ok(DaemonHandle {
    task,
    socket_path: socket_path.to_path_buf(),
    _server_handle: server_handle,
  })
}
