use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
}

impl TestEnv {
  pub fn new() -> Self {
    Self {
      temp: TempDir::new().expect("temp dir"),
    }
  }

  pub fn path(&self) -> &std::path::Path {
    self.temp.path()
  }

  pub fn bin_cmd(&self) -> anyhow::Result<Command> {
    let mut cmd = Command::cargo_bin("agency")?;
    cmd.current_dir(self.path());
    Ok(cmd)
  }
}
