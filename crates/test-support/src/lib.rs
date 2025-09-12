use http_body_util::BodyExt;
use hyperlocal::UnixClientExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Temporary workspace root for tests.
/// Provides convenience helpers for common layout and git initialization.
pub struct TempAgency {
  pub root: tempfile::TempDir,
}

impl Default for TempAgency {
  fn default() -> Self {
    Self::new()
  }
}

impl TempAgency {
  pub fn new() -> Self {
    let root = tempfile::tempdir().expect("tempdir");
    Self { root }
  }

  pub fn path(&self) -> PathBuf {
    self.root.path().to_path_buf()
  }

  /// Initialize a git repository with user config only (no commit).
  pub fn init_git(&self) -> git2::Repository {
    init_repo_only(self.path())
  }

  /// Create `.agency` directory inside the temp root.
  pub fn mkdir_agency(&self) -> PathBuf {
    let p = self.path().join(".agency");
    std::fs::create_dir_all(&p).expect("mkdir .agency");
    p
  }
}

/// Initialize a git repository at `path` and configure user.
pub fn init_repo_only<P: AsRef<Path>>(path: P) -> git2::Repository {
  let repo = git2::Repository::init(path.as_ref()).expect("init git");
  let mut cfg = repo.config().unwrap();
  cfg.set_str("user.name", "Test").unwrap();
  cfg.set_str("user.email", "test@example.com").unwrap();
  repo
}

/// Initialize a repo with an initial commit on `main` and set HEAD.
pub fn init_repo_with_initial_commit<P: AsRef<Path>>(path: P) -> git2::Repository {
  let repo = init_repo_only(&path);
  // Write a file and commit
  let path_ref = path.as_ref();
  std::fs::write(path_ref.join("README.md"), "hello").unwrap();
  let mut idx = repo.index().unwrap();
  idx.add_path(Path::new("README.md")).unwrap();
  idx.write().unwrap();
  let tree_id = idx.write_tree().unwrap();
  let oid = {
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = repo.signature().unwrap();
    repo
      .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
      .unwrap()
  };
  let _ = repo.branch("main", &repo.find_commit(oid).unwrap(), true);
  repo.set_head("refs/heads/main").unwrap();
  repo
}

/// Poll a condition repeatedly until it returns true or times out.
/// Returns true if condition met, false on timeout.
pub async fn poll_until<F, Fut>(timeout: Duration, interval: Duration, mut check: F) -> bool
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = bool>,
{
  use tokio::time::{Instant, sleep};
  let start = Instant::now();
  loop {
    if check().await {
      return true;
    }
    if start.elapsed() >= timeout {
      return false;
    }
    sleep(interval).await;
  }
}

/// Minimal JSON-RPC 2.0 response wrapper for tests.
#[derive(Debug, serde::Deserialize)]
pub struct RpcError {
  pub code: i32,
  pub message: String,
  pub data: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RpcResp<T> {
  pub jsonrpc: String,
  pub id: serde_json::Value,
  pub result: Option<T>,
  pub error: Option<RpcError>,
}

/// A tiny Unix-domain JSON-RPC client used by tests.
pub struct UnixRpcClient {
  sock: PathBuf,
}

impl UnixRpcClient {
  pub fn new<P: AsRef<Path>>(sock: P) -> Self {
    Self {
      sock: sock.as_ref().to_path_buf(),
    }
  }

  fn build_request(
    &self,
    body: serde_json::Value,
  ) -> hyper::Request<http_body_util::Full<hyper::body::Bytes>> {
    let url = hyperlocal::Uri::new(&self.sock, "/");
    hyper::Request::builder()
      .method(hyper::Method::POST)
      .uri(url)
      .header(hyper::header::CONTENT_TYPE, "application/json")
      .body(http_body_util::Full::<hyper::body::Bytes>::from(
        serde_json::to_vec(&body).unwrap(),
      ))
      .unwrap()
  }

  pub async fn call<T: serde::de::DeserializeOwned>(
    &self,
    method: &str,
    params: Option<serde_json::Value>,
  ) -> RpcResp<T> {
    let req_body = serde_json::json!({
      "jsonrpc": "2.0",
      "id": 1,
      "method": method,
      "params": params
    });
    let req = self.build_request(req_body);
    let client = hyper_util::client::legacy::Client::unix();
    let resp = client.request(req).await.expect("request ok");
    assert!(resp.status().is_success());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).expect("valid json")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn git_init_with_commit_creates_main() {
    let td = tempfile::tempdir().unwrap();
    let repo = init_repo_with_initial_commit(td.path());
    // Ensure HEAD points to main
    let head = repo.head().unwrap();
    assert_eq!(head.name(), Some("refs/heads/main"));
  }
}
