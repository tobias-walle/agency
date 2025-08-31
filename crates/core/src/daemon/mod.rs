use std::path::{Path, PathBuf};
use std::{fs, io};

use bytes::Bytes;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper::service::service_fn;
use hyper::server::conn::http1;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::rpc::DaemonStatus;

/// Handle to the running daemon server.
pub struct DaemonHandle {
  task: JoinHandle<()>,
  socket_path: PathBuf,
}

impl DaemonHandle {
  /// Stop the daemon task and remove the socket file if it exists.
  pub fn stop(self) {
    self.task.abort();
    let _ = fs::remove_file(&self.socket_path);
  }

  /// Get the socket path the daemon is bound to.
  pub fn socket_path(&self) -> &Path {
    &self.socket_path
  }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
  #[allow(dead_code)]
  jsonrpc: Option<String>,
  method: String,
  #[allow(dead_code)]
  params: Option<serde_json::Value>,
  id: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse<'a, T> {
  jsonrpc: &'a str,
  id: serde_json::Value,
  #[serde(skip_serializing_if = "Option::is_none")]
  result: Option<T>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
  code: i64,
  message: String,
}

/// Start a minimal HTTP JSON-RPC server over a Unix domain socket.
/// Currently supports method `daemon.status`.
pub async fn start(socket_path: &Path) -> io::Result<DaemonHandle> {
  if let Some(parent) = socket_path.parent() {
    fs::create_dir_all(parent)?;
  }
  // Remove stale socket if present
  let _ = fs::remove_file(socket_path);

  let listener = UnixListener::bind(socket_path)?;
  let sock = socket_path.to_path_buf();

  info!(event = "daemon_started", socket = %socket_path.display(), "daemon server started");

  let task = tokio::spawn(async move {
    loop {
      match listener.accept().await {
        Ok((stream, _addr)) => {
          tokio::spawn(handle_conn(stream, sock.clone()));
        }
        Err(e) => {
          error!(error = %e, "accept error");
          break;
        }
      }
    }
  });

  Ok(DaemonHandle { task, socket_path: socket_path.to_path_buf() })
}

async fn handle_conn(stream: UnixStream, socket_path: PathBuf) {
  let service = service_fn(move |req: Request<IncomingBody>| {
    let socket_path = socket_path.clone();
    async move { handle_request(req, &socket_path).await }
  });

  let io = TokioIo::new(stream);
  if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
    error!(error = %e, "serve_connection error");
  }
}

async fn handle_request(
  req: Request<IncomingBody>,
  socket_path: &Path,
) -> Result<Response<Full<Bytes>>, hyper::http::Error> {
  if req.method() != hyper::Method::POST {
    return Response::builder()
      .status(StatusCode::METHOD_NOT_ALLOWED)
      .body(Full::from(Bytes::from_static(b"method not allowed")));
  }

  let whole = match req.into_body().collect().await {
    Ok(collected) => collected.to_bytes(),
    Err(_e) => {
      return Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .body(Full::from(Bytes::from_static(b"invalid body")));
    }
  };

  let rpc: Result<JsonRpcRequest, _> = serde_json::from_slice(&whole);
  match rpc {
    Ok(r) => match r.method.as_str() {
      "daemon.status" => {
        let status = DaemonStatus {
          version: env!("CARGO_PKG_VERSION").to_string(),
          pid: std::process::id(),
          socket_path: socket_path.display().to_string(),
        };
        info!(event = "daemon_status", pid = status.pid, socket = %status.socket_path, version = %status.version, "status served");
        let resp = JsonRpcResponse { jsonrpc: "2.0", id: r.id, result: Some(status), error: None };
        let bytes = serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec());
        Response::builder()
          .status(StatusCode::OK)
          .header(hyper::header::CONTENT_TYPE, "application/json")
          .body(Full::from(Bytes::from(bytes)))
      }
      _ => {
        let err = JsonRpcError { code: -32601, message: "Method not found".to_string() };
        let resp: JsonRpcResponse<serde_json::Value> = JsonRpcResponse { jsonrpc: "2.0", id: r.id, result: None, error: Some(err) };
        let bytes = serde_json::to_vec(&resp).unwrap_or_else(|_| b"{}".to_vec());
        Response::builder()
          .status(StatusCode::OK)
          .header(hyper::header::CONTENT_TYPE, "application/json")
          .body(Full::from(Bytes::from(bytes)))
      }
    },
    Err(_e) => Response::builder()
      .status(StatusCode::BAD_REQUEST)
      .body(Full::from(Bytes::from_static(b"invalid json"))),
  }
}
