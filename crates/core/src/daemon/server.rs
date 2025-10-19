use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jsonrpsee::server::{self, RpcModule};
use tokio::net::UnixListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Create a shutdown channel for coordinating server termination.
pub fn shutdown_channel() -> (watch::Sender<bool>, watch::Receiver<bool>) {
  watch::channel(false)
}

/// Start the JSON-RPC server accept loop bound to a Unix domain socket.
/// Returns the spawned task handle and the server handle to keep the server alive.
pub fn start(
  socket_path: &Path,
  module: RpcModule<PathBuf>,
  mut shutdown_rx: watch::Receiver<bool>,
) -> io::Result<(JoinHandle<()>, server::ServerHandle)> {
  if let Some(parent) = socket_path.parent() {
    fs::create_dir_all(parent)?;
  }
  // Remove stale socket if present
  let _ = fs::remove_file(socket_path);

  let listener = UnixListener::bind(socket_path)?;
  let sock = socket_path.to_path_buf();

  let svc_builder = server::Server::builder().to_service_builder();
  let (stop_handle, server_handle) = server::stop_channel();

  info!(event = "daemon_started", socket = %socket_path.display(), "daemon server started");

  let task = tokio::spawn(async move {
    loop {
      tokio::select! {
        _ = shutdown_rx.changed() => {
          info!(event = "daemon_shutdown", "shutdown signal received; stopping accept loop");
          break;
        }
        res = listener.accept() => {
          match res {
            Ok((stream, _addr)) => {
              let methods = module.clone();
              let svc = svc_builder.clone().build(methods, stop_handle.clone());
              // Serve the UnixStream (HTTP over UDS)
              tokio::spawn(async move {
                if let Err(e) = server::serve(stream, svc).await {
                  error!(error = %e, "serve error");
                }
              });
            }
            Err(e) => {
              error!(error = %e, "accept error");
              break;
            }
          }
        }
      }
    }
    // Best-effort cleanup
    let _ = fs::remove_file(&sock);
    info!(event = "daemon_stopped", socket = %sock.display(), "daemon server stopped");
  });

  Ok((task, server_handle))
}
