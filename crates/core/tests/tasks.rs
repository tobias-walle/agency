use std::path::Path;
use std::time::Duration;

use agency_core::{
  adapters::fs as fsutil,
  domain::task::{Agent, Status, Task},
  logging,
  rpc::{
    TaskInfo, TaskListParams, TaskListResponse, TaskNewParams, TaskRef, TaskStartParams,
    TaskStartResult,
  },
};
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
struct RpcError {
  #[allow(dead_code)]
  code: i32,
  #[allow(dead_code)]
  message: String,
  #[allow(dead_code)]
  data: Option<Value>,
}

#[derive(serde::Deserialize)]
struct RpcResp<T> {
  #[allow(dead_code)]
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
  let req_body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
  let req = build_request(sock, req_body);
  let client = Client::unix();
  let resp = client.request(req).await.expect("request ok");
  assert!(resp.status().is_success());
  let bytes = resp.into_body().collect().await.unwrap().to_bytes();
  serde_json::from_slice(&bytes).expect("valid json")
}

async fn start_test_env() -> TestEnv {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  // init logs once per test file
  let log = fsutil::logs_path(&root);
  logging::init(&log, agency_core::config::LogLevel::Info);

  // init git repo with an initial commit on main
  let repo = git2::Repository::init(&root).unwrap();
  {
    let mut cfg = repo.config().unwrap();
    cfg.set_str("user.name", "Test").unwrap();
    cfg.set_str("user.email", "test@example.com").unwrap();
  }
  // create initial commit
  std::fs::write(root.join("README.md"), "hello").unwrap();
  let mut idx = repo.index().unwrap();
  idx.add_path(std::path::Path::new("README.md")).unwrap();
  idx.write().unwrap();
  let tree_id = idx.write_tree().unwrap();
  let tree = repo.find_tree(tree_id).unwrap();
  let sig = repo.signature().unwrap();
  let oid = repo
    .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
    .unwrap();
  // ensure main exists
  let _ = repo.branch("main", &repo.find_commit(oid).unwrap(), true);
  // set HEAD to main
  repo.set_head("refs/heads/main").unwrap();

  let sock = td.path().join("agency.sock");
  let handle = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");
  tokio::time::sleep(Duration::from_millis(100)).await;

  TestEnv {
    _td: td,
    root,
    sock,
    handle,
  }
}

#[test]
fn stopped_idle_transitions_are_distinct() {
  assert!(Task::can_transition(&Status::Running, &Status::Stopped));
  assert!(Task::can_transition(&Status::Stopped, &Status::Running));
  assert!(!Task::can_transition(&Status::Idle, &Status::Stopped));
  assert!(!Task::can_transition(&Status::Stopped, &Status::Idle));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_new_and_list_and_start_flow() {
  let env = start_test_env().await;

  // Create a new task
  let params = TaskNewParams {
    project_root: env.root.display().to_string(),
    slug: "feat-x".into(),
    base_branch: "main".into(),
    labels: vec!["a".into()],
    agent: Agent::Fake,
    body: Some("do things".into()),
  };
  let v: RpcResp<TaskInfo> = rpc_call(
    &env.sock,
    "task.new",
    Some(serde_json::to_value(&params).unwrap()),
  )
  .await;
  assert!(v.error.is_none(), "unexpected error: {:?}", v.error);
  let info = v.result.unwrap();
  assert_eq!(info.slug, "feat-x");
  assert_eq!(info.status, Status::Draft);
  assert!(info.id > 0);

  // List tasks
  let list_params = TaskListParams {
    project_root: env.root.display().to_string(),
  };
  let l: RpcResp<TaskListResponse> = rpc_call(
    &env.sock,
    "task.status",
    Some(serde_json::to_value(&list_params).unwrap()),
  )
  .await;
  assert!(l.error.is_none());
  let resp = l.result.unwrap();
  assert_eq!(resp.tasks.len(), 1);
  assert_eq!(resp.tasks[0].slug, "feat-x");

  // Start the task (stub): validates git and flips to running
  let start_params = TaskStartParams {
    project_root: env.root.display().to_string(),
    task: TaskRef {
      id: Some(info.id),
      slug: None,
    },
  };
  let s: RpcResp<TaskStartResult> = rpc_call(
    &env.sock,
    "task.start",
    Some(serde_json::to_value(&start_params).unwrap()),
  )
  .await;
  assert!(s.error.is_none(), "start error: {:?}", s.error);
  let sr = s.result.unwrap();
  assert_eq!(sr.status, Status::Running);

  env.handle.stop();
}
