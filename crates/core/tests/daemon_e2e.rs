use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::client::legacy::Client;
use hyperlocal::UnixClientExt;
use orchestra_core::{adapters::fs as fsutil, logging, rpc::DaemonStatus};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

struct TestEnv {
  // Keep the socket tempdir alive for test duration
  _td: tempfile::TempDir,
  log_path: PathBuf,
  sock: PathBuf,
  handle: orchestra_core::daemon::DaemonHandle,
}

static LOG_DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

fn ensure_logging_once() -> PathBuf {
  if let Some(p) = LOG_PATH.get() {
    return p.clone();
  }
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let log_path = fsutil::logs_path(&root);
  logging::init(&log_path, orchestra_core::config::LogLevel::Info);
  let _ = LOG_DIR.set(td); // keep tempdir alive for process lifetime
  let _ = LOG_PATH.set(log_path.clone());
  log_path
}

async fn start_test_env() -> TestEnv {
  let log_path = ensure_logging_once();

  let td = tempfile::tempdir().unwrap();
  let sock = td.path().join("orchestra.sock");
  let handle = orchestra_core::daemon::start(&sock)
    .await
    .expect("start daemon");

  // small delay to ensure server is listening
  tokio::time::sleep(Duration::from_millis(200)).await;

  TestEnv {
    _td: td,
    log_path,
    sock,
    handle,
  }
}

fn build_request(sock: &Path, body: Value) -> Request<Full<Bytes>> {
  let url = hyperlocal::Uri::new(sock, "/");
  Request::builder()
    .method(Method::POST)
    .uri(url)
    .header(hyper::header::CONTENT_TYPE, "application/json")
    .body(Full::<Bytes>::from(serde_json::to_vec(&body).unwrap()))
    .unwrap()
}

#[derive(Debug, serde::Deserialize)]
struct RpcError {
  code: i32,
  message: String,
  #[allow(dead_code)]
  data: Option<Value>,
}

#[derive(serde::Deserialize)]
struct RpcResp<T> {
  jsonrpc: String,
  #[allow(dead_code)]
  id: Value,
  result: Option<T>,
  error: Option<RpcError>,
}

async fn rpc_call<T: DeserializeOwned>(
  sock: &Path,
  method: &str,
  params: Option<Value>,
) -> RpcResp<T> {
  let req_body = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": method,
    "params": params
  });
  let req = build_request(sock, req_body);
  let client = Client::unix();
  let resp = client.request(req).await.expect("request ok");
  assert!(resp.status().is_success());
  let bytes = resp.into_body().collect().await.unwrap().to_bytes();
  serde_json::from_slice(&bytes).expect("valid json")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_status_roundtrip() {
  let env = start_test_env().await;

  let v: RpcResp<DaemonStatus> = rpc_call(&env.sock, "daemon.status", None).await;
  assert_eq!(v.jsonrpc, "2.0");
  assert!(v.error.is_none(), "unexpected error: {:?}", v.error);
  let status = v.result.expect("has result");
  assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
  assert!(status.pid > 0);
  assert_eq!(status.socket_path, env.sock.display().to_string());

  // Best-effort: allow logs to flush and check we logged the event if this test owns the logger.
  tokio::time::sleep(Duration::from_millis(100)).await;
  if let Ok(log_text) = std::fs::read_to_string(&env.log_path)
    && !log_text.is_empty()
  {
    assert!(
      log_text.contains("daemon_status"),
      "missing daemon_status log entry; logs: {}",
      log_text
    );
  }

  // Cleanup
  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_method_returns_error() {
  let env = start_test_env().await;

  let v: RpcResp<Value> = rpc_call(&env.sock, "daemon.nope", None).await;
  assert_eq!(v.jsonrpc, "2.0");
  assert!(v.result.is_none());
  let err = v.error.expect("should have error");
  // jsonrpsee uses standard JSON-RPC codes; -32601 is Method not found
  assert_eq!(err.code, -32601);
  assert!(err.message.to_lowercase().contains("method"));

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handles_multiple_connections() {
  let env = start_test_env().await;

  // Fire a few concurrent calls
  let t1 = rpc_call::<DaemonStatus>(&env.sock, "daemon.status", None);
  let t2 = rpc_call::<DaemonStatus>(&env.sock, "daemon.status", None);
  let t3 = rpc_call::<DaemonStatus>(&env.sock, "daemon.status", None);

  let (r1, r2, r3) = tokio::join!(t1, t2, t3);

  for r in [r1, r2, r3] {
    assert!(r.error.is_none());
    let s = r.result.unwrap();
    assert_eq!(s.version, env!("CARGO_PKG_VERSION"));
    assert!(s.pid > 0);
    assert_eq!(s.socket_path, env.sock.display().to_string());
  }

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_via_rpc_stops_server() {
  let env = start_test_env().await;

  // Request shutdown
  let v: RpcResp<serde_json::Value> = rpc_call(&env.sock, "daemon.shutdown", None).await;
  assert!(v.error.is_none());
  assert!(v.result.is_some());

  // Wait a moment for shutdown to take effect
  tokio::time::sleep(Duration::from_millis(200)).await;

  // Subsequent call should fail at HTTP layer or return JSON-RPC error; emulate by trying to connect
  let req = build_request(
    &env.sock,
    json!({
      "jsonrpc": "2.0",
      "id": 1,
      "method": "daemon.status",
      "params": null
    }),
  );
  let client = Client::unix();
  let res = client.request(req).await;
  assert!(res.is_err(), "server should be down");
}
