use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use agency_core::{
  adapters::fs as fsutil,
  domain::task::{Status, Task, TaskId},
  logging,
  rpc::{
    DaemonStatus,
    TaskInfo,
    TaskListResponse,
    TaskNewParams,
    TaskRef,
    TaskStartParams,
    TaskStartResult,
  },
};
use hyperlocal::UnixClientExt;
use serde_json::{Value, json};
use test_support::{RpcResp, UnixRpcClient, init_repo_with_initial_commit, poll_until};

struct TestEnv {
  _td: tempfile::TempDir,
  log_path: PathBuf,
  sock: PathBuf,
  handle: agency_core::daemon::DaemonHandle,
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
  logging::init(&log_path, agency_core::config::LogLevel::Info);
  let _ = LOG_DIR.set(td);
  let _ = LOG_PATH.set(log_path.clone());
  log_path
}

async fn start_test_env() -> TestEnv {
  let log_path = ensure_logging_once();

  let td = tempfile::tempdir().unwrap();
  let sock = td.path().join("agency.sock");
  let handle = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");

  // Poll until daemon answers status instead of fixed sleep
  let client = UnixRpcClient::new(&sock);
  let ok = poll_until(Duration::from_secs(2), Duration::from_millis(50), || {
    let c = &client;
    async move {
      let r: RpcResp<DaemonStatus> = c.call("daemon.status", None).await;
      r.error.is_none()
    }
  })
  .await;
  assert!(ok, "daemon did not become ready in time");

  TestEnv {
    _td: td,
    log_path,
    sock,
    handle,
  }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_status_roundtrip() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  let v: RpcResp<DaemonStatus> = client.call("daemon.status", None).await;
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

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_method_returns_error() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  let v: RpcResp<Value> = client.call("daemon.nope", None).await;
  assert_eq!(v.jsonrpc, "2.0");
  assert!(v.result.is_none());
  let err = v.error.expect("should have error");
  assert_eq!(err.code, -32601);
  assert!(err.message.to_lowercase().contains("method"));

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handles_multiple_connections() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  let t1 = client.call::<DaemonStatus>("daemon.status", None);
  let t2 = client.call::<DaemonStatus>("daemon.status", None);
  let t3 = client.call::<DaemonStatus>("daemon.status", None);
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
async fn marks_task_stopped_when_agent_exits() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let log = fsutil::logs_path(&root);
  logging::init(&log, agency_core::config::LogLevel::Info);
  fsutil::ensure_layout(&root).unwrap();
  std::fs::write(
    root.join(".agency/config.toml"),
    r#"[agents.fake]
start = ["sh", "-c", "exit 0"]
"#,
  )
  .unwrap();

  let sock = td.path().join("agency.sock");
  let handle = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");

  let client = UnixRpcClient::new(&sock);
  let ready = poll_until(Duration::from_secs(2), Duration::from_millis(50), || {
    let c = &client;
    async move {
      let r: RpcResp<DaemonStatus> = c.call("daemon.status", None).await;
      r.error.is_none()
    }
  })
  .await;
  assert!(ready, "daemon did not become ready in time");

  let params = TaskNewParams {
    project_root: root.display().to_string(),
    slug: "auto-stop".into(),
    base_branch: "main".into(),
    labels: vec![],
    agent: agency_core::domain::task::Agent::Fake,
    body: None,
  };
  let created: RpcResp<TaskInfo> = client
    .call("task.new", Some(serde_json::to_value(&params).unwrap()))
    .await;
  assert!(created.error.is_none(), "task.new error: {:?}", created.error);
  let info = created.result.unwrap();

  init_repo_with_initial_commit(&root);

  let start_params = TaskStartParams {
    project_root: root.display().to_string(),
    task: TaskRef {
      id: Some(info.id),
      slug: None,
    },
  };
  let started: RpcResp<TaskStartResult> = client
    .call("task.start", Some(serde_json::to_value(&start_params).unwrap()))
    .await;
  assert!(started.error.is_none(), "task.start error: {:?}", started.error);

  let task_id = info.id;
  let root_str = root.display().to_string();
  let stopped = poll_until(Duration::from_secs(3), Duration::from_millis(100), || {
    let c = &client;
    let root_clone = root_str.clone();
    async move {
      let status: RpcResp<TaskListResponse> = c
        .call("task.status", Some(json!({ "project_root": root_clone })))
        .await;
      if let Some(result) = status.result {
        return result
          .tasks
          .into_iter()
          .find(|t| t.id == task_id)
          .map(|t| t.status == Status::Stopped)
          .unwrap_or(false);
      }
      false
    }
  })
  .await;
  assert!(stopped, "task did not transition to stopped after agent exit");

  let task_path = fsutil::tasks_dir(&root)
    .join(Task::format_filename(TaskId(task_id), &info.slug));
  let contents = std::fs::read_to_string(&task_path).unwrap();
  let parsed = Task::from_markdown(TaskId(task_id), info.slug.clone(), &contents).unwrap();
  assert_eq!(parsed.front_matter.status, Status::Stopped);

  handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_via_rpc_stops_server() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  let v: RpcResp<serde_json::Value> = client.call("daemon.shutdown", None).await;
  assert!(v.error.is_none());
  assert!(v.result.is_some());

  // Subsequent call should fail at HTTP layer; perform a raw HTTP request
  let url = hyperlocal::Uri::new(&env.sock, "/");
  let req = hyper::Request::builder()
    .method(hyper::Method::POST)
    .uri(url)
    .header(hyper::header::CONTENT_TYPE, "application/json")
    .body(http_body_util::Full::<hyper::body::Bytes>::from(
      serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "daemon.status",
        "params": null
      }))
      .unwrap(),
    ))
    .unwrap();
  let raw_client = hyper_util::client::legacy::Client::unix();
  let res = raw_client.request(req).await;
  assert!(res.is_err(), "server should be down");
}
