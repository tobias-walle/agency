use std::path::PathBuf;

pub struct TempAgency {
  pub root: tempfile::TempDir,
}

impl Default for TempAgency {
  fn default() -> Self {
    Self::new()
  }
}

impl TempAgency {
  pub fn new() -> Self {
    let root = tempfile::tempdir().expect("tempdir");
    Self { root }
  }

  pub fn path(&self) -> PathBuf {
    self.root.path().to_path_buf()
  }

  pub fn init_git(&self) -> git2::Repository {
    let repo = git2::Repository::init(self.path()).expect("init git");
    // Configure user to avoid libgit2 complaining on commit
    {
      let mut cfg = repo.config().unwrap();
      cfg.set_str("user.name", "Test").unwrap();
      cfg.set_str("user.email", "test@example.com").unwrap();
    }
    repo
  }

  pub fn mkdir_agency(&self) -> PathBuf {
    let p = self.path().join(".agency");
    std::fs::create_dir_all(&p).expect("mkdir .agency");
    p
  }
}
