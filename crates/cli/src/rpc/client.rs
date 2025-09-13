use std::path::Path;

use agency_core::rpc::{
  DaemonStatus, PtyAttachParams, PtyAttachResult, PtyDetachParams, PtyInputParams, PtyReadResult,
  PtyResizeParams, TaskInfo, TaskListParams, TaskListResponse, TaskNewParams, TaskRef,
  TaskStartParams, TaskStartResult,
};
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, body::Bytes};
use hyper_util::client::legacy::{Client, Error as LegacyClientError};
use hyperlocal::UnixClientExt;
use serde_json::json;
use tracing::debug;

/// Session wrapper for reusing HTTP client across attach operations
pub struct PtySession {
  client: Client<hyperlocal::UnixConnector, Full<Bytes>>,
}

impl PtySession {
  pub fn new() -> Self {
    Self {
      client: Client::unix(),
    }
  }

  pub async fn rpc_call(
    &self,
    sock: &Path,
    method: &str,
    params: Option<serde_json::Value>,
  ) -> Result<serde_json::Value> {
    let url = hyperlocal::Uri::new(sock, "/");
    let req_body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let req = Request::builder()
      .method(Method::POST)
      .uri(url)
      .header(hyper::header::CONTENT_TYPE, "application/json")
      .body(Full::<Bytes>::from(serde_json::to_vec(&req_body)?))
      .unwrap();

    let resp = self.client.request(req).await?;
    let status_code = resp.status();
    let bytes = resp.into_body().collect().await?.to_bytes();
    if !status_code.is_success() {
      return Err(Error::HttpStatus(
        status_code.as_u16(),
        String::from_utf8_lossy(&bytes).into(),
      ));
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(err) = v.get("error") {
      let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(-32000) as i32;
      let message = match err.get("message").and_then(|m| m.as_str()) {
        Some(m) => m.to_string(),
        None => err.to_string(),
      };
      let data = err.get("data").cloned();
      return Err(Error::Rpc {
        code,
        message,
        data,
      });
    }
    let result = v.get("result").cloned().ok_or_else(|| Error::Rpc {
      code: -32000,
      message: "missing result".to_string(),
      data: None,
    })?;
    Ok(result)
  }
}

impl Default for PtySession {
  fn default() -> Self {
    Self::new()
  }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("http: {0}")]
  Http(#[from] hyper::Error),
  #[error("client: {0}")]
  Client(#[from] LegacyClientError),
  #[error("json: {0}")]
  Json(#[from] serde_json::Error),
  #[error("rpc {code}: {message}")]
  Rpc {
    code: i32,
    message: String,
    data: Option<serde_json::Value>,
  },
  #[error("http status {0}: {1}")]
  HttpStatus(u16, String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub async fn pty_tick(
  sock: &Path,
  attachment_id: &str,
  input: Option<&str>,
  resize: Option<(u16, u16)>,
  max_bytes: Option<usize>,
  wait_ms: Option<u64>,
) -> Result<PtyReadResult> {
  let params = serde_json::json!({
    "attachment_id": attachment_id,
    "input": input,
    "resize": resize,
    "max_bytes": max_bytes,
    "wait_ms": wait_ms,
  });
  debug!(event = "rpc_pty_tick_call", wait_ms, input_len = input.map(|s| s.len()), resize = ?resize, max_bytes);
  let v = rpc_call(sock, "pty.tick", Some(params)).await?;
  let res: PtyReadResult = serde_json::from_value(v)?;
  debug!(
    event = "rpc_pty_tick_resp",
    data_len = res.data.len(),
    eof = res.eof
  );
  Ok(res)
}

async fn rpc_call(
  sock: &Path,
  method: &str,
  params: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
  let session = PtySession::new();
  session.rpc_call(sock, method, params).await
}

pub async fn daemon_status(sock: &Path) -> Result<DaemonStatus> {
  let v = rpc_call(sock, "daemon.status", None).await?;
  let status: DaemonStatus = serde_json::from_value(v)?;
  Ok(status)
}

pub async fn daemon_shutdown(sock: &Path) -> Result<()> {
  let _ = rpc_call(sock, "daemon.shutdown", None).await?;
  Ok(())
}

// ---- Task wrappers ----

pub async fn task_new(sock: &Path, params: TaskNewParams) -> Result<TaskInfo> {
  let v = rpc_call(sock, "task.new", Some(serde_json::to_value(params)?)).await?;
  let info: TaskInfo = serde_json::from_value(v)?;
  Ok(info)
}

pub async fn task_status(sock: &Path, project_root: &Path) -> Result<TaskListResponse> {
  let params = TaskListParams {
    project_root: project_root.display().to_string(),
  };
  let v = rpc_call(sock, "task.status", Some(serde_json::to_value(params)?)).await?;
  let resp: TaskListResponse = serde_json::from_value(v)?;
  Ok(resp)
}

pub async fn task_start(
  sock: &Path,
  project_root: &Path,
  task: TaskRef,
) -> Result<TaskStartResult> {
  let params = TaskStartParams {
    project_root: project_root.display().to_string(),
    task,
  };
  let v = rpc_call(sock, "task.start", Some(serde_json::to_value(params)?)).await?;
  let res: TaskStartResult = serde_json::from_value(v)?;
  Ok(res)
}

// ---- PTY wrappers ----

pub async fn pty_attach(
  sock: &Path,
  project_root: &Path,
  task: TaskRef,
  rows: u16,
  cols: u16,
) -> Result<PtyAttachResult> {
  pty_attach_with_replay(sock, project_root, task, rows, cols, true).await
}

pub async fn pty_attach_with_replay(
  sock: &Path,
  project_root: &Path,
  task: TaskRef,
  rows: u16,
  cols: u16,
  replay: bool,
) -> Result<PtyAttachResult> {
  let params = serde_json::to_value(PtyAttachParams {
    project_root: project_root.display().to_string(),
    task,
    rows,
    cols,
    replay: Some(replay),
  })?;
  let v = rpc_call(sock, "pty.attach", Some(params)).await?;
  let res: PtyAttachResult = serde_json::from_value(v)?;
  Ok(res)
}

pub async fn pty_read(
  sock: &Path,
  attachment_id: &str,
  max_bytes: Option<usize>,
) -> Result<PtyReadResult> {
  let params = serde_json::json!({ "attachment_id": attachment_id, "max_bytes": max_bytes, "wait_ms": serde_json::Value::Null });
  debug!(event = "rpc_pty_read_call", max_bytes);
  let v = rpc_call(sock, "pty.read", Some(params)).await?;
  let res: PtyReadResult = serde_json::from_value(v)?;
  debug!(
    event = "rpc_pty_read_resp",
    data_len = res.data.len(),
    eof = res.eof
  );
  Ok(res)
}

pub async fn pty_input(sock: &Path, attachment_id: &str, data: &[u8]) -> Result<()> {
  debug!(event = "rpc_pty_input_call", bytes = data.len());
  let params = serde_json::to_value(PtyInputParams {
    attachment_id: attachment_id.to_string(),
    data: String::from_utf8_lossy(data).to_string(),
  })?;
  let _ = rpc_call(sock, "pty.input", Some(params)).await?;
  Ok(())
}

pub async fn pty_resize(sock: &Path, attachment_id: &str, rows: u16, cols: u16) -> Result<()> {
  let params = serde_json::to_value(PtyResizeParams {
    attachment_id: attachment_id.to_string(),
    rows,
    cols,
  })?;
  let _ = rpc_call(sock, "pty.resize", Some(params)).await?;
  Ok(())
}

pub async fn pty_detach(sock: &Path, attachment_id: &str) -> Result<()> {
  let params = serde_json::to_value(PtyDetachParams {
    attachment_id: attachment_id.to_string(),
  })?;
  let _ = rpc_call(sock, "pty.detach", Some(params)).await?;
  Ok(())
}

/// Session-based PTY operations for attach loop efficiency
pub mod session {
  use super::*;

  pub async fn pty_read(
    session: &PtySession,
    sock: &Path,
    attachment_id: &str,
    max_bytes: Option<usize>,
  ) -> Result<PtyReadResult> {
    let params = serde_json::json!({ "attachment_id": attachment_id, "max_bytes": max_bytes, "wait_ms": serde_json::Value::Null });
    let v = session.rpc_call(sock, "pty.read", Some(params)).await?;
    let res: PtyReadResult = serde_json::from_value(v)?;
    Ok(res)
  }

  pub async fn pty_read_wait(
    session: &PtySession,
    sock: &Path,
    attachment_id: &str,
    max_bytes: Option<usize>,
    wait_ms: Option<u64>,
  ) -> Result<PtyReadResult> {
    let params = serde_json::json!({ "attachment_id": attachment_id, "max_bytes": max_bytes, "wait_ms": wait_ms });
    let v = session.rpc_call(sock, "pty.read", Some(params)).await?;
    let res: PtyReadResult = serde_json::from_value(v)?;
    Ok(res)
  }

  pub async fn pty_input(
    session: &PtySession,
    sock: &Path,
    attachment_id: &str,
    data: &[u8],
  ) -> Result<()> {
    let params = serde_json::to_value(PtyInputParams {
      attachment_id: attachment_id.to_string(),
      data: String::from_utf8_lossy(data).to_string(),
    })?;
    let _ = session.rpc_call(sock, "pty.input", Some(params)).await?;
    Ok(())
  }

  pub async fn pty_resize(
    session: &PtySession,
    sock: &Path,
    attachment_id: &str,
    rows: u16,
    cols: u16,
  ) -> Result<()> {
    let params = serde_json::to_value(PtyResizeParams {
      attachment_id: attachment_id.to_string(),
      rows,
      cols,
    })?;
    let _ = session.rpc_call(sock, "pty.resize", Some(params)).await?;
    Ok(())
  }

  pub async fn pty_detach(session: &PtySession, sock: &Path, attachment_id: &str) -> Result<()> {
    let params = serde_json::to_value(PtyDetachParams {
      attachment_id: attachment_id.to_string(),
    })?;
    let _ = session.rpc_call(sock, "pty.detach", Some(params)).await?;
    Ok(())
  }
}
