use crate::common::test_env::TestEnv;
use anyhow::{Context, Result};
use std::path::PathBuf;

pub const TMUX_SESSION_NAME: &str = "agency_test";

impl TestEnv {
  pub fn tmux_tests_socket_path(&self) -> PathBuf {
    self.runtime_dir().join("agency-tests-tmux.sock")
  }

  pub fn tmux_new_session(&self, args: &[&str]) -> Result<()> {
    let mut cmd = std::process::Command::new("tmux");
    cmd
      .arg("-S")
      .arg(self.tmux_tests_socket_path())
      .arg("new-session")
      .arg("-d")
      .arg("-c")
      .arg(self.path())
      .arg("-s")
      .arg(TMUX_SESSION_NAME);
    if !args.is_empty() {
      cmd.args(args);
    }
    let output = cmd.output().context("tmux new-session for test failed")?;
    if output.status.success() {
      return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
      "tmux new-session for test exited with {} stderr: {}",
      output.status,
      stderr
    );
  }

  pub fn tmux_send_keys(&self, keys: &str) -> Result<()> {
    let output = std::process::Command::new("tmux")
      .arg("-S")
      .arg(self.tmux_tests_socket_path())
      .arg("send-keys")
      .arg("-t")
      .arg(TMUX_SESSION_NAME)
      .arg(keys)
      .output()
      .context("tmux send-keys for test failed")?;
    if output.status.success() {
      return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
      "tmux send-keys for test exited with {} stderr: {}",
      output.status,
      stderr
    );
  }

  pub fn tmux_capture_pane(&self) -> Result<String> {
    let output = std::process::Command::new("tmux")
      .arg("-S")
      .arg(self.tmux_tests_socket_path())
      .arg("capture-pane")
      .arg("-p")
      .arg("-J")
      .arg("-t")
      .arg(TMUX_SESSION_NAME)
      .output()
      .context("tmux capture-pane for test failed")?;
    if !output.status.success() {
      let stderr = String::from_utf8_lossy(&output.stderr);
      anyhow::bail!(
        "tmux capture-pane for test exited with {} stderr: {}",
        output.status,
        stderr
      );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
  }
}
