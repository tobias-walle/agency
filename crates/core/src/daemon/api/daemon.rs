use std::path::PathBuf;

use jsonrpsee::core::RpcResult;
use jsonrpsee::server::RpcModule;
use tokio::sync::watch;
use tracing::info;

use crate::rpc::DaemonStatus;

/// Register daemon.status and daemon.shutdown APIs.
pub fn register(module: &mut RpcModule<PathBuf>, shutdown_tx: watch::Sender<bool>) {
  module
    .register_method("daemon.status", |_params, ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let status = DaemonStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        socket_path: ctx.display().to_string(),
      };
      info!(event = "daemon_status", pid = status.pid, socket = %status.socket_path, version = %status.version, "status served");
      Ok(serde_json::json!(status))
    })
    .expect("register daemon.status");

  let shutdown_tx_for_shutdown = shutdown_tx.clone();
  module
    .register_method(
      "daemon.shutdown",
      move |_params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        info!(
          event = "daemon_shutdown_requested",
          "shutdown requested via RPC"
        );
        let _ = shutdown_tx_for_shutdown.send(true);
        Ok(serde_json::json!(true))
      },
    )
    .expect("register daemon.shutdown");
}
