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
}
