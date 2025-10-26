use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgencyConfig {
  cwd: PathBuf,
}

impl AgencyConfig {
  pub fn new(cwd: impl Into<PathBuf>) -> Self {
    Self { cwd: cwd.into() }
  }

  pub fn cwd(&self) -> &PathBuf {
    &self.cwd
  }

  pub fn tasks_dir(&self) -> PathBuf {
    self.cwd.join(".agency").join("tasks")
  }

  pub fn worktrees_dir(&self) -> PathBuf {
    self.cwd.join(".agency").join("worktrees")
  }
}
