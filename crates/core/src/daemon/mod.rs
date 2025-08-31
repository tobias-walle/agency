use std::path::{Path, PathBuf};
use std::{fs, io};

use jsonrpsee::core::RpcResult;
use jsonrpsee::server::{self, RpcModule};
use tokio::net::UnixListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::rpc::DaemonStatus;

/// Handle to the running daemon server.
pub struct DaemonHandle {
  task: JoinHandle<()>,
  socket_path: PathBuf,
  // Keep the server handle alive to prevent immediate shutdown
  _server_handle: server::ServerHandle,
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
/// Supports methods `daemon.status` and `daemon.shutdown`.
pub async fn start(socket_path: &Path) -> io::Result<DaemonHandle> {
  if let Some(parent) = socket_path.parent() {
    fs::create_dir_all(parent)?;
  }
  // Remove stale socket if present
  let _ = fs::remove_file(socket_path);

  let listener = UnixListener::bind(socket_path)?;
  let sock = socket_path.to_path_buf();

  // Shutdown signal channel
  let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

  // Build jsonrpsee module with context of the socket path
  let mut module = RpcModule::new(sock.clone());
  module
    .register_method("daemon.status", |_params, ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let status = DaemonStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        socket_path: ctx.display().to_string(),
      };
      info!(event = "daemon_status", pid = status.pid, socket = %status.socket_path, version = %status.version, "status served");
      Ok(serde_json::to_value(status).unwrap())
    })
    .expect("register daemon.status");

  let (stop_handle, _server_handle) = server::stop_channel();
  let _stop_handle_for_shutdown = stop_handle.clone();
  let shutdown_tx_for_shutdown = shutdown_tx.clone();

  module
    .register_method("daemon.shutdown", move |_params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      info!(event = "daemon_shutdown_requested", "shutdown requested via RPC");
      // Signal accept loop to exit; existing connections will drain
      let _ = shutdown_tx_for_shutdown.send(true);
      Ok(serde_json::json!(true))
    })
    .expect("register daemon.shutdown");

  let svc_builder = server::Server::builder().to_service_builder();

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

  Ok(DaemonHandle { task, socket_path: socket_path.to_path_buf(), _server_handle })
}
