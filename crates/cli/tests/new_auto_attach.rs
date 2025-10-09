use assert_cmd::prelude::*;
use std::process::Command;
use test_support::init_repo_with_initial_commit;

type RpcResp<T> = test_support::RpcResp<T>;

#[derive(Debug, serde::Deserialize)]
struct TaskListResponse {
  tasks: Vec<TaskInfo>,
}

#[derive(Debug, serde::Deserialize)]
struct TaskInfo {
  id: u64,
  slug: String,
  status: serde_json::Value,
}

#[test]
fn new_skips_auto_attach_when_stdout_not_tty() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  init_repo_with_initial_commit(&root);

  Command::cargo_bin("agency")
    .expect("compile bin")
    .env("AGENCY_SOCKET", &sock)
    .current_dir(&root)
    .args(["daemon", "start"])
    .assert()
    .success();

  let output = Command::cargo_bin("agency")
    .expect("compile bin")
    .env("AGENCY_SOCKET", &sock)
    .current_dir(&root)
    .args(["new", "--agent", "fake", "-m", "Body", "feat-auto"])
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let stdout = String::from_utf8_lossy(&output);
  assert!(stdout.contains("feat-auto"), "stdout: {}", stdout);
  assert!(stdout.contains("Running"), "stdout: {}", stdout);

  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap();
  let client = test_support::UnixRpcClient::new(&sock);

  let tasks: TaskListResponse = rt.block_on(async {
    let resp: RpcResp<TaskListResponse> = client
      .call(
        "task.status",
        Some(serde_json::json!({
          "project_root": root.display().to_string()
        })),
      )
      .await;
    assert!(resp.error.is_none(), "task.status error: {:?}", resp.error);
    resp.result.unwrap()
  });

  let task = tasks
    .tasks
    .iter()
    .find(|t| t.slug == "feat-auto" && t.status == serde_json::json!("running"))
    .expect("task should be running");
  assert_eq!(task.id, 1);

  Command::cargo_bin("agency")
    .expect("compile bin")
    .env("AGENCY_SOCKET", &sock)
    .current_dir(&root)
    .args(["daemon", "stop"])
    .assert()
    .success();
}
