use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyperlocal::UnixClientExt;
use orchestra_core::{adapters::fs as fsutil, logging, rpc::DaemonStatus};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_status_roundtrip() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path();
  let log_path = fsutil::logs_path(root);
  logging::init(&log_path, orchestra_core::config::LogLevel::Info);

  let sock = root.join("orchestra.sock");
  let handle = orchestra_core::daemon::start(&sock).await.expect("start daemon");

  // Give the server a brief moment
  tokio::time::sleep(Duration::from_millis(20)).await;

  let req_body = serde_json::json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "daemon.status",
    "params": null
  });
  let body_bytes = serde_json::to_vec(&req_body).unwrap();

  // Build a hyperlocal client request
  let url = hyperlocal::Uri::new(sock.clone(), "/");
  let req = Request::builder()
    .method(Method::POST)
    .uri(url)
    .header(hyper::header::CONTENT_TYPE, "application/json")
    .body(Full::<Bytes>::from(body_bytes))
    .unwrap();

  let client = Client::unix();
  let resp = client.request(req).await.expect("request ok");
  assert!(resp.status().is_success());
  let bytes = resp.into_body().collect().await.unwrap().to_bytes();

  #[derive(serde::Deserialize)]
  struct RpcResp {
    jsonrpc: String,
    id: serde_json::Value,
    result: Option<DaemonStatus>,
    error: Option<serde_json::Value>,
  }
  let v: RpcResp = serde_json::from_slice(&bytes).expect("valid json");
  assert_eq!(v.jsonrpc, "2.0");
  assert!(v.error.is_none(), "unexpected error: {:?}", v.error);
  let status = v.result.expect("has result");
  assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
  assert!(status.pid > 0);
  assert_eq!(status.socket_path, sock.display().to_string());

  // Allow logs to flush
  tokio::time::sleep(Duration::from_millis(50)).await;
  let log_text = std::fs::read_to_string(&log_path).expect("read logs");
  assert!(log_text.contains("daemon_status"), "missing daemon_status log entry; logs: {}", log_text);

  // Cleanup
  handle.stop();
}
