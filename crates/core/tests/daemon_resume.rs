use std::path::Path;
use std::time::Duration;

use agency_core::rpc::{PtyAttachResult, PtyReadResult, TaskInfo, TaskNewParams, TaskRef, TaskStartParams, TaskStartResult};
use agency_core::{adapters::fs as fsutil, domain::task::Agent, logging};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_util::client::legacy::Client;
use hyperlocal::UnixClientExt;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

struct TestEnv {
  _td: tempfile::TempDir,
  root: std::path::PathBuf,
  sock: std::path::PathBuf,
  handle: agency_core::daemon::DaemonHandle,
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
struct RpcError { #[allow(dead_code)] code: i32, #[allow(dead_code)] message: String, #[allow(dead_code)] data: Option<Value> }

#[derive(serde::Deserialize)]
struct RpcResp<T> { #[allow(dead_code)] jsonrpc: String, #[allow(dead_code)] id: Value, result: Option<T>, error: Option<RpcError> }

async fn rpc_call<T: DeserializeOwned>(sock: &Path, method: &str, params: Option<Value>) -> RpcResp<T> {
  let req_body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
  let req = build_request(sock, req_body);
  let client = Client::unix();
  let resp = client.request(req).await.expect("request ok");
  assert!(resp.status().is_success());
  let bytes = resp.into_body().collect().await.unwrap().to_bytes();
  serde_json::from_slice(&bytes).expect("valid json")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resumes_running_task_on_boot() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let log = fsutil::logs_path(&root);
  logging::init(&log, agency_core::config::LogLevel::Info);
  let sock = td.path().join("agency.sock");

  // First daemon lifetime
  let handle1 = agency_core::daemon::start(&sock).await.expect("start daemon");
  tokio::time::sleep(Duration::from_millis(150)).await;

  // Create and start a task
  let params = TaskNewParams { project_root: root.display().to_string(), slug: "feat-resume".into(), base_branch: "main".into(), labels: vec![], agent: Agent::Fake, body: None };
  let v: RpcResp<TaskInfo> = rpc_call(&sock, "task.new", Some(serde_json::to_value(&params).unwrap())).await;
  assert!(v.error.is_none());
  let info = v.result.unwrap();

  // init git repo required for task.start
  let repo = git2::Repository::init(&root).unwrap();
  {
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", "Test").unwrap();
    cfg.set_str("user.email", "test@example.com").unwrap();
  }
  std::fs::write(root.join("README.md"), "hello").unwrap();
  let mut idx = repo.index().unwrap();
  idx.add_path(std::path::Path::new("README.md")).unwrap();
  idx.write().unwrap();
  let tree_id = idx.write_tree().unwrap();
  let tree = repo.find_tree(tree_id).unwrap();
  let sig = repo.signature().unwrap();
  let oid = repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[]).unwrap();
  let _ = repo.branch("main", &repo.find_commit(oid).unwrap(), true);
  repo.set_head("refs/heads/main").unwrap();

  let start_params = TaskStartParams { project_root: root.display().to_string(), task: TaskRef { id: Some(info.id), slug: None } };
  let s: RpcResp<TaskStartResult> = rpc_call(&sock, "task.start", Some(serde_json::to_value(&start_params).unwrap())).await;
  assert!(s.error.is_none());

  // Stop first daemon
  handle1.stop();

  // Set resume root and start a new daemon instance
  unsafe { std::env::set_var("AGENCY_RESUME_ROOT", &root); }
  let handle2 = agency_core::daemon::start(&sock).await.expect("start daemon again");
  tokio::time::sleep(Duration::from_millis(200)).await;

  // Attach without calling task.start again
  let attach_params = json!({
    "project_root": root.display().to_string(),
    "task": {"id": info.id},
    "rows": 24u16,
    "cols": 80u16
  });
  let att: RpcResp<PtyAttachResult> = rpc_call(&sock, "pty.attach", Some(attach_params)).await;
  assert!(att.error.is_none(), "attach error: {:?}", att.error);

  // Basic read sanity
  let attachment_id = att.result.unwrap().attachment_id;
  let r: RpcResp<PtyReadResult> = rpc_call(&sock, "pty.read", Some(json!({
    "attachment_id": attachment_id,
    "max_bytes": 4096usize,
    "wait_ms": 50u64
  }))).await;
  assert!(r.error.is_none());

  handle2.stop();
}
