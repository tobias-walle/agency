use assert_cmd::prelude::*;
use serde::Deserialize;
use std::process::Command;
use test_support::init_repo_with_initial_commit;

#[derive(Debug, Deserialize)]
struct TaskInfo {
  id: u64,
  slug: String,
  status: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TaskListResponse {
  tasks: Vec<TaskInfo>,
}

#[derive(Debug, Deserialize)]
struct PtyAttachResult { attachment_id: String }

#[derive(Debug, Deserialize)]
struct PtyReadResult { data: String, eof: bool }

#[test]
fn new_opencode_auto_attaches_and_injects_command_into_pty_history() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");

  // Initialize repo with a commit on main
  let _repo = init_repo_with_initial_commit(&root);

  // Prepare a mock `opencode` in PATH
  let bindir = root.join("bin");
  std::fs::create_dir_all(&bindir).unwrap();
  let mock_path = bindir.join("opencode");
  std::fs::write(&mock_path, b"#!/bin/sh\necho MOCK_OPENCODE\n").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&mock_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&mock_path, perms).unwrap();
  }
  let path_prefix = format!("{}:{}", bindir.display(), std::env::var("PATH").unwrap_or_default());

  // Start daemon (ensure it inherits the PATH with our mock)
  let mut start = Command::cargo_bin("agency").expect("compile bin");
  start
    .env("AGENCY_SOCKET", &sock)
    .env("PATH", &path_prefix)
    .current_dir(&root)
    .args(["daemon", "start"]) // wait for running
    .assert()
    .success();

  // Run `new` with opencode agent WITHOUT --no-attach; include a message body
  let mut newc = Command::cargo_bin("agency").expect("compile bin");
  let out = newc
    .env("AGENCY_SOCKET", &sock)
    .env("PATH", &path_prefix)
    .current_dir(&root)
    .args(["new", "--agent", "opencode", "-m", "Body", "feat-inject"]) // auto-attach should happen
    .assert()
    .success()
    .get_output()
    .stdout
    .clone();
  let stdout = String::from_utf8_lossy(&out);
  assert!(stdout.contains("feat-inject"), "stdout: {}", stdout);

  // Query task list to find the id for the slug
  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_io()
    .enable_time()
    .build()
    .unwrap();

  let client = test_support::UnixRpcClient::new(&sock);

  // Fetch tasks and find feat-inject
  let tasks: TaskListResponse = rt
    .block_on(async {
      let resp: test_support::RpcResp<TaskListResponse> = client
        .call("task.status", Some(serde_json::json!({
          "project_root": root.display().to_string()
        })))
        .await;
      assert!(resp.error.is_none(), "rpc error: {:?}", resp.error);
      resp.result.unwrap()
    });
  let task = tasks
    .tasks
    .into_iter()
    .find(|t| t.slug == "feat-inject")
    .expect("task present");

  // Attach with replay to read history that should include our injected command echo
  let attach_res: PtyAttachResult = rt.block_on(async {
    let resp: test_support::RpcResp<PtyAttachResult> = client
      .call(
        "pty.attach",
        Some(serde_json::json!({
          "project_root": root.display().to_string(),
          "task": { "id": task.id },
          "rows": 24u16,
          "cols": 80u16,
          "replay": true
        })),
      )
      .await;
    assert!(resp.error.is_none(), "attach error: {:?}", resp.error);
    resp.result.unwrap()
  });

  // Read a chunk of replay (allow some time for command to run)
  let mut combined = String::new();
  for _ in 0..30 { // up to ~3s total
    let read: PtyReadResult = rt.block_on(async {
      let resp: test_support::RpcResp<PtyReadResult> = client
        .call(
          "pty.read",
          Some(serde_json::json!({
            "attachment_id": attach_res.attachment_id,
            "max_bytes": 65536usize,
            "wait_ms": 100u64
          })),
        )
        .await;
      assert!(resp.error.is_none(), "read error: {:?}", resp.error);
      resp.result.unwrap()
    });
    combined.push_str(&read.data);
    if combined.contains("MOCK_OPENCODE") {
      break;
    }
  }

  // The mocked opencode output should be visible in the PTY history replay
  assert!(
    combined.contains("MOCK_OPENCODE"),
    "expected MOCK_OPENCODE in replay, got: {}",
    combined
  );
}
