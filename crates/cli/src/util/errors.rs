use crate::rpc::client;
use std::path::Path;

pub fn render_rpc_failure(action: &str, sock: &Path, err: &client::Error) -> String {
  match err {
    client::Error::Client(_) | client::Error::Http(_) => format!(
      "{} failed: daemon not reachable at {}.",
      action,
      sock.display()
    ),
    _ => format!("{} failed: {}", action, err),
  }
}
