use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::Result;

use crate::{log_info, log_warn};

/// Spawn a child process with unified I/O routing.
///
/// - When a TUI log sink is set: route stdout lines as Info and stderr lines as Warn.
///   Stdin is set to null to avoid stealing focus from the TUI.
/// - Without a sink: inherit stdio to preserve regular CLI behavior.
pub fn run_child_process(
  program: &str,
  args: &[String],
  cwd: &Path,
  env: &[(String, String)],
) -> Result<ExitStatus> {
  if crate::utils::log::is_sink_set() {
    let mut cmd = Command::new(program);
    cmd
      .current_dir(cwd)
      .args(args)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped());
    // Extend environment without clearing the existing one
    for (key, value) in env.iter() {
      cmd.env(key, value);
    }

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_handle = if let Some(out) = stdout {
      Some(std::thread::spawn(move || {
        let reader = BufReader::new(out);
        for line in reader.lines().flatten() {
          log_info!("{}", line);
        }
      }))
    } else {
      None
    };

    let stderr_handle = if let Some(err) = stderr {
      Some(std::thread::spawn(move || {
        let reader = BufReader::new(err);
        for line in reader.lines().flatten() {
          log_warn!("{}", line);
        }
      }))
    } else {
      None
    };

    let status = child.wait()?;
    if let Some(h) = stdout_handle {
      let _ = h.join();
    }
    if let Some(h) = stderr_handle {
      let _ = h.join();
    }
    Ok(status)
  } else {
    let mut cmd = Command::new(program);
    cmd
      .current_dir(cwd)
      .args(args)
      .stdin(Stdio::inherit())
      .stdout(Stdio::inherit())
      .stderr(Stdio::inherit());
    for (key, value) in env.iter() {
      cmd.env(key, value);
    }
    Ok(cmd.status()?)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::utils::log::{LogEvent, LogLevel, clear_log_sink, set_log_sink};
  use crossbeam_channel::unbounded;
  use std::env;

  #[test]
  fn routes_stdout_and_stderr_when_sink_set() {
    let (tx, rx) = unbounded();
    set_log_sink(tx);
    let cwd = env::current_dir().unwrap();
    let program = "/bin/sh";
    let args = vec![
      "-c".to_string(),
      "echo out1; echo err1 1>&2; echo out2; echo err2 1>&2".to_string(),
    ];
    let status = run_child_process(program, &args, &cwd, &[]).expect("run child");
    assert!(status.success());

    let events: Vec<LogEvent> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
      events.len() >= 4,
      "expected >=4 events, got {}",
      events.len()
    );
    // Validate that both Info and Warn lines were emitted
    let mut info_count = 0;
    let mut warn_count = 0;
    for ev in events {
      if let LogEvent::Line { level, ansi } = ev {
        match level {
          LogLevel::Info => {
            info_count += 1;
            assert!(ansi.contains("out"));
          }
          LogLevel::Warn => {
            warn_count += 1;
            assert!(ansi.contains("err"));
          }
          _ => {}
        }
      }
    }
    assert!(info_count >= 2, "info_count {}", info_count);
    assert!(warn_count >= 2, "warn_count {}", warn_count);
    clear_log_sink();
  }

  #[test]
  fn inherits_stdio_without_sink() {
    clear_log_sink();
    let cwd = env::current_dir().unwrap();
    let program = "/bin/sh";
    let args = vec!["-c".to_string(), "exit 0".to_string()];
    let status = run_child_process(program, &args, &cwd, &[]).expect("run child");
    assert!(status.success());
  }
}
