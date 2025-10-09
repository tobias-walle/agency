use std::time::Duration;

use agency_core::rpc::{
  PtyAttachResult, TaskInfo, TaskListResponse, TaskNewParams, TaskRef, TaskStartParams,
  TaskStartResult,
};
use agency_core::{adapters::fs as fsutil, domain::task::{Agent, Status}, logging};
use serde_json::{Value, json};
use test_support::{RpcResp, UnixRpcClient, init_repo_with_initial_commit, poll_until};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn marks_running_task_as_stopped_on_boot() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let log = fsutil::logs_path(&root);
  logging::init(&log, agency_core::config::LogLevel::Info);
  let sock = td.path().join("agency.sock");

  // First daemon lifetime
  let handle1 = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");

  // Poll until ready
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

  // Create and start a task
  let params = TaskNewParams {
    project_root: root.display().to_string(),
    slug: "feat-resume".into(),
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

  // init git repo required for task.start
  let _repo = init_repo_with_initial_commit(&root);

  let start_params = TaskStartParams {
    project_root: root.display().to_string(),
    task: TaskRef {
      id: Some(info.id),
      slug: None,
    },
  };
  let s: RpcResp<TaskStartResult> = client
    .call(
      "task.start",
      Some(serde_json::to_value(&start_params).unwrap()),
    )
    .await;
  assert!(s.error.is_none());

  // Stop first daemon
  handle1.stop();

  // Set resume root and start a new daemon instance
  unsafe {
    std::env::set_var("AGENCY_RESUME_ROOT", &root);
  }
  let handle2 = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon again");

  // Poll until ready again
  let client2 = UnixRpcClient::new(&sock);
  let ok2 = poll_until(Duration::from_secs(2), Duration::from_millis(50), || {
    let c = &client2;
    async move {
      let r: RpcResp<Value> = c.call("daemon.status", None).await;
      r.error.is_none()
    }
  })
  .await;
  assert!(ok2, "daemon did not become ready in time after restart");

  let status_resp: RpcResp<TaskListResponse> = client2
    .call(
      "task.status",
      Some(json!({ "project_root": root.display().to_string() })),
    )
    .await;
  assert!(status_resp.error.is_none(), "status error: {:?}", status_resp.error);
  let tasks = status_resp.result.unwrap().tasks;
  assert_eq!(tasks.len(), 1);
  assert_eq!(tasks[0].status, Status::Stopped);

  let attach_params = json!({
    "project_root": root.display().to_string(),
    "task": {"id": info.id},
    "rows": 24u16,
    "cols": 80u16
  });
  let att: RpcResp<PtyAttachResult> = client2
    .call("pty.attach", Some(attach_params.clone()))
    .await;
  assert!(att.error.is_some(), "expected attach to fail when task is stopped");

  let restart: RpcResp<TaskStartResult> = client2
    .call(
      "task.start",
      Some(serde_json::to_value(&start_params).unwrap()),
    )
    .await;
  assert!(restart.error.is_none(), "restart start error: {:?}", restart.error);
  let restart_info = restart.result.unwrap();
  assert_eq!(restart_info.status, Status::Running);

  let att2: RpcResp<PtyAttachResult> = client2
    .call("pty.attach", Some(attach_params))
    .await;
  assert!(att2.error.is_none(), "attach after restart error: {:?}", att2.error);

  handle2.stop();
  unsafe {
    std::env::remove_var("AGENCY_RESUME_ROOT");
  }
}
