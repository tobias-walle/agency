use std::fs::{self, OpenOptions};
use std::path::Path;
use std::sync::OnceLock;

use crate::config::LogLevel;
use tracing::{info, subscriber::set_global_default};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::ChronoUtc;
use tracing_subscriber::{EnvFilter, Registry, fmt, layer::SubscriberExt};

static WORKER_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Initialize structured JSON logging to the given `logs.jsonl` path.
/// This function is idempotent in practice (subsequent calls will return an error
/// from `set_global_default` which we ignore). It ensures the parent directory exists.
pub fn init(logs_path: &Path, level: LogLevel) {
  if let Some(parent) = logs_path.parent() {
    let _ = fs::create_dir_all(parent);
  }

  // Open file in append mode, create if missing
  let file = OpenOptions::new()
    .create(true)
    .append(true)
    .open(logs_path)
    .expect("open logs.jsonl for append");

  // Non-blocking writer to avoid stalling on disk IO. Keep guard alive globally.
  let (nb_writer, guard) = tracing_appender::non_blocking(file);
  let _ = WORKER_GUARD.set(guard);

  // Map config level to tracing filter
  let filter = EnvFilter::new(match level {
    LogLevel::Off => "off",
    LogLevel::Warn => "warn",
    LogLevel::Info => "info",
    LogLevel::Debug => "debug",
    LogLevel::Trace => "trace",
  });

  let json_layer = fmt::layer()
    .with_timer(ChronoUtc::rfc_3339())
    .json()
    .with_current_span(true)
    .with_span_list(true)
    .with_level(true)
    .with_target(false)
    .with_thread_ids(false)
    .with_thread_names(false)
    .with_writer(move || nb_writer.clone());

  let subscriber = Registry::default().with(filter).with(json_layer);

  // Ignore error if already set
  let _ = set_global_default(subscriber);

  info!(
    event = "logging_initialized",
    logs_path = %logs_path.display(),
    level = ?level,
    "logging initialized"
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::Value;
  use std::{fs, thread, time::Duration};
  use tracing::info;

  #[test]
  fn writes_json_logs() {
    let td = tempfile::tempdir().unwrap();
    let log = td.path().join("logs.jsonl");

    init(&log, LogLevel::Info);
    info!(answer = 42, "hello world");

    // Allow background worker to flush
    thread::sleep(Duration::from_millis(50));

    let s = fs::read_to_string(&log).expect("read logs");
    assert!(s.lines().count() >= 1, "no log lines written");

    // Find an initialized line
    let mut saw_init = false;
    let mut saw_event = false;
    for line in s.lines() {
      if let Ok(v) = serde_json::from_str::<Value>(line) {
        // basic shape
        assert!(v.get("timestamp").is_some());
        assert!(v.get("level").is_some());
        if v
          .get("fields")
          .and_then(|f| f.get("event"))
          .and_then(|e| e.as_str())
          == Some("logging_initialized")
        {
          saw_init = true;
        }
        if v
          .get("fields")
          .and_then(|f| f.get("message"))
          .and_then(|m| m.as_str())
          == Some("hello world")
        {
          saw_event = true;
        }
      }
    }
    assert!(saw_init, "missing logging_initialized event");
    assert!(saw_event, "missing hello world event");
  }
}
