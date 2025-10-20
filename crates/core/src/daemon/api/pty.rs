use std::path::PathBuf;

use jsonrpsee::core::RpcResult;
use jsonrpsee::server::RpcModule;
use jsonrpsee::types::ErrorObjectOwned;
use tracing::debug;

use crate::domain::task::{Status, Task, TaskId};
use crate::rpc::{
  KeyCombinationDTO, PtyAttachParams, PtyAttachResult, PtyDetachParams, PtyInputEventsParams,
  PtyInputParams, PtyReadParams, PtyReadResult, PtyResizeParams,
};

use super::super::task_index::find_task_path_by_ref;

/// Register PTY-related APIs: attach, read, tick, input, resize, detach.
pub fn register(module: &mut RpcModule<PathBuf>) {
  // ---- pty.attach ----
  module
    .register_method("pty.attach", |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let p: PtyAttachParams = params.parse()?;
      let root = PathBuf::from(&p.project_root);
      let (path, id, _slug) = find_task_path_by_ref(&root, &p.task)
        .map_err(|e| ErrorObjectOwned::owned(-32001, e.to_string(), None::<()>))?;
      // Enforce running state; do not spawn here
      let s = std::fs::read_to_string(&path)
        .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      let task = Task::from_markdown(TaskId(id), "_".into(), &s)
        .map_err(|e| ErrorObjectOwned::owned(-32000, e.to_string(), None::<()>))?;
      if task.front_matter.status != Status::Running {
        return Err(ErrorObjectOwned::owned(-32010, format!("cannot attach: task is not running (status: {:?})", task.front_matter.status), None::<()>));
      }
      // Attach to existing session
      let prefill = p.replay.unwrap_or(true);
      let attach_id = crate::adapters::pty::attach(&root, id, prefill)
        .map_err(|e| ErrorObjectOwned::owned(-32010, e.to_string(), None::<()>))?;
      // Apply initial size with a minimal jiggle to force redraws in TUIs
      let _ = crate::adapters::pty::jiggle_resize(&attach_id, p.rows, p.cols);
      tracing::info!(event = "pty_attach_jiggle_resize", task_id = id, attachment_id = %attach_id, rows = p.rows, cols = p.cols, "applied initial jiggle resize");
      tracing::info!(event = "pty_attach", task_id = id, attachment_id = %attach_id, rows = p.rows, cols = p.cols, "pty attached");
      let res = PtyAttachResult { attachment_id: attach_id };
      Ok(serde_json::json!(res))
    })
    .expect("register pty.attach");

  // ---- pty.read ----
  module
    .register_method(
      "pty.read",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyReadParams = params.parse()?;
        let (data, eof) = crate::adapters::pty::read(&p.attachment_id, p.max_bytes, p.wait_ms)
          .map_err(|e| ErrorObjectOwned::owned(-32011, e.to_string(), None::<()>))?;
        let text = String::from_utf8_lossy(&data).to_string();
        let res = PtyReadResult { data: text, eof };
        Ok(serde_json::json!(res))
      },
    )
    .expect("register pty.read");

  // ---- pty.tick ----
  module
    .register_method(
      "pty.tick",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: crate::rpc::PtyTickParams = params.parse()?;
        if let Some(ref input) = p.input {
          debug!(event = "daemon_pty_tick_input", attachment_id = %p.attachment_id, bytes = input.len());
          crate::adapters::pty::input(&p.attachment_id, input.as_bytes())
            .map_err(|e| ErrorObjectOwned::owned(-32012, e.to_string(), None::<()>))?;
        }
        if let Some((rows, cols)) = p.resize {
          debug!(event = "daemon_pty_tick_resize", attachment_id = %p.attachment_id, rows, cols);
          crate::adapters::pty::resize(&p.attachment_id, rows, cols)
            .map_err(|e| ErrorObjectOwned::owned(-32013, e.to_string(), None::<()>))?;
        }
        let (data, eof) = crate::adapters::pty::read(&p.attachment_id, p.max_bytes, p.wait_ms)
          .map_err(|e| ErrorObjectOwned::owned(-32011, e.to_string(), None::<()>))?;
        debug!(event = "daemon_pty_tick_read", attachment_id = %p.attachment_id, bytes = data.len(), eof, wait_ms = p.wait_ms, max_bytes = ?p.max_bytes);
        let text = String::from_utf8_lossy(&data).to_string();
        let res = PtyReadResult { data: text, eof };
        Ok(serde_json::json!(res))
      },
    )
    .expect("register pty.tick");

  // ---- pty.input ----
  module
    .register_method(
      "pty.input",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyInputParams = params.parse()?;
        debug!(event = "daemon_pty_input", attachment_id = %p.attachment_id, bytes = p.data.len());
        crate::adapters::pty::input(&p.attachment_id, p.data.as_bytes())
          .map_err(|e| ErrorObjectOwned::owned(-32012, e.to_string(), None::<()>))?;
        Ok(serde_json::json!(true))
      },
    )
    .expect("register pty.input");

  // ---- pty.input_events ----
  module
    .register_method(
      "pty.input_events",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyInputEventsParams = params.parse()?;
        let bytes: Vec<u8> = p
          .events
          .into_iter()
          .flat_map(|ev: KeyCombinationDTO| crate::adapters::pty::input_encode::encode_event(&ev))
          .collect();
        debug!(event = "daemon_pty_input_events", attachment_id = %p.attachment_id, events = bytes.len());
        crate::adapters::pty::input(&p.attachment_id, &bytes)
          .map_err(|e| ErrorObjectOwned::owned(-32012, e.to_string(), None::<()>))?;
        Ok(serde_json::json!(true))
      },
    )
    .expect("register pty.input_events");

  // ---- pty.resize ----
  module
    .register_method("pty.resize", |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
      let p: PtyResizeParams = params.parse()?;
      crate::adapters::pty::resize(&p.attachment_id, p.rows, p.cols)
        .map_err(|e| ErrorObjectOwned::owned(-32013, e.to_string(), None::<()>))?;
      tracing::info!(event = "pty_resize", attachment_id = %p.attachment_id, rows = p.rows, cols = p.cols, "pty resized");
      Ok(serde_json::json!(true))
    })
    .expect("register pty.resize");

  // ---- pty.detach ----
  module
    .register_method(
      "pty.detach",
      |params, _ctx: &PathBuf, _ext| -> RpcResult<serde_json::Value> {
        let p: PtyDetachParams = params.parse()?;
        crate::adapters::pty::detach(&p.attachment_id)
          .map_err(|e| ErrorObjectOwned::owned(-32014, e.to_string(), None::<()>))?;
        tracing::info!(event = "pty_detach", attachment_id = %p.attachment_id, "pty detached");
        Ok(serde_json::json!(true))
      },
    )
    .expect("register pty.detach");
}
