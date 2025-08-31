use std::path::Path;

use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, Method, Request};
use hyper_util::client::legacy::{Client, Error as LegacyClientError};
use hyperlocal::UnixClientExt;
use orchestra_core::rpc::DaemonStatus;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("http: {0}")]
  Http(#[from] hyper::Error),
  #[error("client: {0}")]
  Client(#[from] LegacyClientError),
  #[error("json: {0}")]
  Json(#[from] serde_json::Error),
  #[error("rpc: {0}")]
  Rpc(String),
  #[error("http status {0}: {1}")]
  HttpStatus(u16, String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub async fn daemon_status(sock: &Path) -> Result<DaemonStatus> {
  let url = hyperlocal::Uri::new(sock, "/");
  let req_body = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "daemon.status",
    "params": null
  });
  let req = Request::builder()
    .method(Method::POST)
    .uri(url)
    .header(hyper::header::CONTENT_TYPE, "application/json")
    .body(Full::<Bytes>::from(serde_json::to_vec(&req_body)?))
    .unwrap();

  let client = Client::unix();
  let resp = client.request(req).await?;
  let status_code = resp.status();
  let bytes = resp.into_body().collect().await?.to_bytes();
  if !status_code.is_success() {
    return Err(Error::HttpStatus(status_code.as_u16(), String::from_utf8_lossy(&bytes).into()));
  }
  let v: serde_json::Value = serde_json::from_slice(&bytes)?;
  if let Some(err) = v.get("error") {
    return Err(Error::Rpc(err.to_string()));
  }
  let result = v
    .get("result")
    .cloned()
    .ok_or_else(|| Error::Rpc("missing result".to_string()))?;
  let status: DaemonStatus = serde_json::from_value(result)?;
  Ok(status)
}
