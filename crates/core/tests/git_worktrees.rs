use std::path::PathBuf;
use std::time::Duration;

use agency_core::{
  adapters::fs as fsutil,
  adapters::git as gitutil,
  domain::task::{Agent, Status},
  logging,
  rpc::{TaskInfo, TaskNewParams, TaskRef, TaskStartParams, TaskStartResult},
};
use test_support::{RpcResp, UnixRpcClient, init_repo_with_initial_commit};

struct TestEnv {
  _td: tempfile::TempDir,
  root: PathBuf,
  sock: PathBuf,
  handle: agency_core::daemon::DaemonHandle,
}

async fn start_test_env_with_repo() -> (TestEnv, git2::Repository) {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let log = fsutil::logs_path(&root);
  logging::init(&log, agency_core::config::LogLevel::Info);

  let repo = init_repo_with_initial_commit(&root);

  let sock = td.path().join("agency.sock");
  let handle = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");
  tokio::time::sleep(Duration::from_millis(100)).await;
  (
    TestEnv {
      _td: td,
      root,
      sock,
      handle,
    },
    repo,
  )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_start_creates_git_worktree_with_branch_at_base() {
  let (env, repo) = start_test_env_with_repo().await;

  // Create a new task
  let params = TaskNewParams {
    project_root: env.root.display().to_string(),
    slug: "feat-x".into(),
    base_branch: "main".into(),
    labels: vec![],
    agent: Agent::Fake,
    body: None,
  };
  let client = UnixRpcClient::new(&env.sock);
  let v: RpcResp<TaskInfo> = client
    .call("task.new", Some(serde_json::to_value(&params).unwrap()))
    .await;
  assert!(v.error.is_none(), "unexpected error: {:?}", v.error);
  let info = v.result.unwrap();

  // Resolve expected worktree path and branch name
  let wt_path = fsutil::worktree_path(&env.root, info.id, &info.slug);
  let branch = gitutil::task_branch_name(info.id, &info.slug);

  // Start the task (should create worktree/branch and spawn PTY)
  let start_params = TaskStartParams {
    project_root: env.root.display().to_string(),
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
  assert!(s.error.is_none(), "start error: {:?}", s.error);
  let sr = s.result.unwrap();
  assert_eq!(sr.status, Status::Running);

  // Assert worktree repo opens and is on the task branch
  let wt_repo = git2::Repository::open(&wt_path).expect("open worktree repo");
  let head = wt_repo.head().expect("wt head");
  assert!(head.is_branch());
  assert_eq!(head.shorthand(), Some(branch.as_str()));

  // Base sha is the tip of base branch in the main repo
  let base_sha = gitutil::resolve_base_branch_tip(&repo, "main").unwrap();
  let head_target = head.target().expect("head target");
  // Newly created branch should point to base sha
  assert_eq!(head_target, base_sha);

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_start_is_idempotent() {
  let (env, _repo) = start_test_env_with_repo().await;

  // Create a task
  let params = TaskNewParams {
    project_root: env.root.display().to_string(),
    slug: "feat-y".into(),
    base_branch: "main".into(),
    labels: vec![],
    agent: Agent::Fake,
    body: None,
  };
  let client = UnixRpcClient::new(&env.sock);
  let v: RpcResp<TaskInfo> = client
    .call("task.new", Some(serde_json::to_value(&params).unwrap()))
    .await;
  assert!(v.error.is_none());
  let info = v.result.unwrap();

  let start_params = TaskStartParams {
    project_root: env.root.display().to_string(),
    task: TaskRef {
      id: Some(info.id),
      slug: None,
    },
  };
  let _s1: RpcResp<TaskStartResult> = client
    .call(
      "task.start",
      Some(serde_json::to_value(&start_params).unwrap()),
    )
    .await;
  let _s2: RpcResp<TaskStartResult> = client
    .call(
      "task.start",
      Some(serde_json::to_value(&start_params).unwrap()),
    )
    .await;

  // Worktree path should still open and branch match expected
  let wt_path = fsutil::worktree_path(&env.root, info.id, &info.slug);
  let branch = gitutil::task_branch_name(info.id, &info.slug);
  let wt_repo = git2::Repository::open(&wt_path).expect("open worktree repo");
  let head = wt_repo.head().expect("wt head");
  assert!(head.is_branch());
  assert_eq!(head.shorthand(), Some(branch.as_str()));

  env.handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_start_errors_outside_git_repo() {
  let td = tempfile::tempdir().unwrap();
  let root = td.path().to_path_buf();
  let sock = td.path().join("agency.sock");
  let handle = agency_core::daemon::start(&sock)
    .await
    .expect("start daemon");
  tokio::time::sleep(Duration::from_millis(50)).await;
  let client = UnixRpcClient::new(&sock);

  // Create layout and task file without git init
  agency_core::adapters::fs::ensure_layout(&root).unwrap();
  let params = TaskNewParams {
    project_root: root.display().to_string(),
    slug: "nogit".into(),
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
  assert!(s.error.is_some(), "expected error outside git repo");
  let err = s.error.unwrap();
  assert!(err.message.to_lowercase().contains("git"));

  handle.stop();
}
