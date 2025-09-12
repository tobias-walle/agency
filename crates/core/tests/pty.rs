use std::time::Duration;

use agency_core::rpc::{
  PtyAttachResult, PtyReadResult, TaskInfo, TaskNewParams, TaskRef, TaskStartParams,
  TaskStartResult,
};
use agency_core::{adapters::fs as fsutil, domain::task::Agent, domain::task::Status, logging};
use serde_json::{Value, json};
use test_support::{init_repo_with_initial_commit, poll_until, RpcResp, UnixRpcClient};

struct TestEnv {
  _td: tempfile::TempDir,
  root: std::path::PathBuf,
  sock: std::path::PathBuf,
  handle: agency_core::daemon::DaemonHandle,
}

async fn start_test_env() -> TestEnv {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let log = fsutil::logs_path(&root);
  logging::init(&log, agency_core::config::LogLevel::Info);

  // init git repo with an initial commit on main
  let _repo = init_repo_with_initial_commit(&root);

  let sock = td.path().join("agency.sock");
  let handle = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");

  // Poll for readiness
  let client = UnixRpcClient::new(&sock);
  let ok = poll_until(Duration::from_secs(2), Duration::from_millis(50), || {
    let c = &client;
    async move {
      let r: RpcResp<Value> = c.call("daemon.status", None).await;
      r.error.is_none()
    }
  })
  .await;
  assert!(ok, "daemon did not become ready in time");

  TestEnv { _td: td, root, sock, handle }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_basic_flow_and_errors() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  // Create a new task
  let params = TaskNewParams {
    project_root: env.root.display().to_string(),
    slug: "feat-pty".into(),
    base_branch: "main".into(),
    labels: vec![],
    agent: Agent::Fake,
    body: None,
  };
  let v: RpcResp<TaskInfo> = client
    .call("task.new", Some(serde_json::to_value(&params).unwrap()))
    .await;
  assert!(v.error.is_none(), "unexpected error: {:?}", v.error);
  let info = v.result.unwrap();
  assert_eq!(info.status, Status::Draft);

  // Attach before start should fail
  let attach_params_draft = json!({
    "project_root": env.root.display().to_string(),
    "task": {"id": info.id},
    "rows": 24u16,
    "cols": 80u16
  });
  let att_draft: RpcResp<PtyAttachResult> = client.call("pty.attach", Some(attach_params_draft)).await;
  assert!(att_draft.error.is_some(), "expected error when attaching draft");

  // Start the task
  let start_params = TaskStartParams {
    project_root: env.root.display().to_string(),
    task: TaskRef { id: Some(info.id), slug: None },
  };
  let s: RpcResp<TaskStartResult> = client
    .call("task.start", Some(serde_json::to_value(&start_params).unwrap()))
    .await;
  assert!(s.error.is_none(), "start error: {:?}", s.error);
  let sr = s.result.unwrap();
  assert_eq!(sr.status, Status::Running);

  // Attach now (with initial size)
  let attach_params = json!({
    "project_root": env.root.display().to_string(),
    "task": {"id": info.id},
    "rows": 24u16,
    "cols": 80u16
  });
  let mut att: RpcResp<PtyAttachResult> = client.call("pty.attach", Some(attach_params)).await;
  assert!(att.error.is_none(), "attach error: {:?}", att.error);
  let attachment_id = att.result.take().unwrap().attachment_id;

  // Double attach should error
  let att2: RpcResp<PtyAttachResult> = client
    .call(
      "pty.attach",
      Some(json!({
        "project_root": env.root.display().to_string(),
        "task": {"id": info.id},
        "rows": 24u16,
        "cols": 80u16
      })),
    )
    .await;
  assert!(att2.error.is_some(), "expected error on double attach");

  // Send input: echo hi\n
  let _in_ok: RpcResp<Value> = client
    .call(
      "pty.input",
      Some(json!({ "attachment_id": attachment_id, "data": "echo hi\n" })),
    )
    .await;

  // Poll read until we see "hi"
  let mut seen = false;
  for _ in 0..40u8 {
    let r: RpcResp<PtyReadResult> = client
      .call(
        "pty.read",
        Some(json!({ "attachment_id": attachment_id, "max_bytes": 8192usize })),
      )
      .await;
    assert!(r.error.is_none(), "read error: {:?}", r.error);
    let data = r.result.as_ref().unwrap().data.clone();
    if data.contains("hi") {
      seen = true;
      break;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  assert!(seen, "PTY output should contain 'hi'");

  // Resize succeeds
  let ok: RpcResp<Value> = client
    .call(
      "pty.resize",
      Some(json!({
        "attachment_id": attachment_id,
        "rows": 40u16,
        "cols": 100u16
      })),
    )
    .await;
  assert!(ok.error.is_none(), "resize should succeed");

  // Detach
  let _det: RpcResp<Value> = client
    .call("pty.detach", Some(json!({ "attachment_id": attachment_id })))
    .await;

  // After detach, read should error
  let r_after: RpcResp<PtyReadResult> = client
    .call(
      "pty.read",
      Some(json!({ "attachment_id": attachment_id, "max_bytes": 1024usize })),
    )
    .await;
  assert!(r_after.error.is_some(), "read after detach should error");

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_reattach_replays_scrollback_tail() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  // Create and start a task
  let params = TaskNewParams {
    project_root: env.root.display().to_string(),
    slug: "feat-scrollback".into(),
    base_branch: "main".into(),
    labels: vec![],
    agent: Agent::Fake,
    body: None,
  };
  let v: RpcResp<TaskInfo> = client
    .call("task.new", Some(serde_json::to_value(&params).unwrap()))
    .await;
  assert!(v.error.is_none());
  let info = v.result.unwrap();

  let start_params = TaskStartParams {
    project_root: env.root.display().to_string(),
    task: TaskRef { id: Some(info.id), slug: None },
  };
  let s: RpcResp<TaskStartResult> = client
    .call("task.start", Some(serde_json::to_value(&start_params).unwrap()))
    .await;
  assert!(s.error.is_none());

  // Attach and send some input to generate output
  let attach_params = json!({
    "project_root": env.root.display().to_string(),
    "task": {"id": info.id},
    "rows": 24u16,
    "cols": 80u16
  });
  let att: RpcResp<PtyAttachResult> = client.call("pty.attach", Some(attach_params.clone())).await;
  assert!(att.error.is_none());
  let attachment_id = att.result.unwrap().attachment_id;

  // Send input to generate output
  let _: RpcResp<Value> = client
    .call(
      "pty.input",
      Some(json!({
        "attachment_id": attachment_id,
        "data": "echo 'line1'\necho 'line2'\necho 'line3'\n"
      })),
    )
    .await;

  // Read and drain the output
  let mut collected = String::new();
  for _ in 0..20 {
    let r: RpcResp<PtyReadResult> = client
      .call(
        "pty.read",
        Some(json!({ "attachment_id": attachment_id, "max_bytes": 8192usize })),
      )
      .await;
    if let Some(res) = r.result {
      collected.push_str(&res.data);
      if res.eof {
        break;
      }
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
  assert!(
    collected.contains("line1") && collected.contains("line2") && collected.contains("line3")
  );

  // Detach
  let _: RpcResp<Value> = client
    .call("pty.detach", Some(json!({ "attachment_id": attachment_id })))
    .await;

  // Re-attach
  let att2: RpcResp<PtyAttachResult> = client.call("pty.attach", Some(attach_params)).await;
  assert!(att2.error.is_none());
  let attachment_id2 = att2.result.unwrap().attachment_id;

  // Read again; should replay the tail of previous output
  let r2: RpcResp<PtyReadResult> = client
    .call(
      "pty.read",
      Some(json!({ "attachment_id": attachment_id2, "max_bytes": 8192usize })),
    )
    .await;
  assert!(r2.error.is_none());
  let replayed = r2.result.unwrap().data;
  assert!(replayed.contains("line3"), "Expected replay of scrollback, got: {}", replayed);

  // Detach again
  let _: RpcResp<Value> = client
    .call("pty.detach", Some(json!({ "attachment_id": attachment_id2 })))
    .await;

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pty_read_wait_ms_returns_on_data() {
  let env = start_test_env().await;
  let client = UnixRpcClient::new(&env.sock);

  // Create and start a task
  let params = TaskNewParams {
    project_root: env.root.display().to_string(),
    slug: "feat-longpoll".into(),
    base_branch: "main".into(),
    labels: vec![],
    agent: Agent::Fake,
    body: None,
  };
  let v: RpcResp<TaskInfo> = client
    .call("task.new", Some(serde_json::to_value(&params).unwrap()))
    .await;
  assert!(v.error.is_none());
  let info = v.result.unwrap();

  let start_params = TaskStartParams {
    project_root: env.root.display().to_string(),
    task: TaskRef { id: Some(info.id), slug: None },
  };
  let s: RpcResp<TaskStartResult> = client
    .call("task.start", Some(serde_json::to_value(&start_params).unwrap()))
    .await;
  assert!(s.error.is_none());

  // Attach
  let attach_params = json!({
    "project_root": env.root.display().to_string(),
    "task": {"id": info.id},
    "rows": 24u16,
    "cols": 80u16
  });
  let att: RpcResp<PtyAttachResult> = client.call("pty.attach", Some(attach_params)).await;
  assert!(att.error.is_none());
  let attachment_id = att.result.unwrap().attachment_id;

  // Spawn a long-poll read that should unblock when input arrives
  use std::time::{Duration, Instant};
  let sock_clone = env.sock.clone();
  let attachment_id_clone = attachment_id.clone();
  let task = tokio::spawn(async move {
    let client = UnixRpcClient::new(&sock_clone);
    let start = Instant::now();
    let r: RpcResp<PtyReadResult> = client
      .call(
        "pty.read",
        Some(json!({
          "attachment_id": attachment_id_clone,
          "max_bytes": 8192usize,
          "wait_ms": 200u64
        })),
      )
      .await;
    (start.elapsed(), r)
  });

  // After a short delay, send input that should wake the long-poll
  tokio::time::sleep(Duration::from_millis(50)).await;
  let _: RpcResp<Value> = client
    .call(
      "pty.input",
      Some(json!({ "attachment_id": attachment_id, "data": "echo ping\n" })),
    )
    .await;

  let (elapsed, r) = task.await.expect("join");
  assert!(r.error.is_none(), "read error: {:?}", r.error);
  let data = r.result.unwrap().data;
  assert!(!data.is_empty(), "expected some output to wake long-poll");
  assert!(elapsed < Duration::from_millis(200), "long-poll should return before timeout (elapsed {:?})", elapsed);

  // Now read until we observe the echoed 'ping'
  let mut seen = data.contains("ping");
  for _ in 0..10 {
    if seen { break; }
    let r2: RpcResp<PtyReadResult> = client
      .call(
        "pty.read",
        Some(json!({
          "attachment_id": attachment_id,
          "max_bytes": 8192usize,
          "wait_ms": 200u64
        })),
      )
      .await;
    assert!(r2.error.is_none());
    let d2 = r2.result.unwrap().data;
    if d2.contains("ping") { seen = true; break; }
  }
  assert!(seen, "expected echoed 'ping' within subsequent reads");

  env.handle.stop();
}
