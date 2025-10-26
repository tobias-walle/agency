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

  pub fn setup_git_repo(&self) -> anyhow::Result<()> {
    let repo = git2::Repository::init(self.path())?;
    // Ensure HEAD points to main
    repo.set_head("refs/heads/main")?;
    Ok(())
  }

  pub fn simulate_initial_commit(&self) -> anyhow::Result<()> {
    use std::fs;
    use std::path::Path;

    let repo = git2::Repository::open(self.path())?;

    // Write a file
    let readme = self.path().join("README.md");
    fs::write(&readme, "init\n")?;

    // Stage it
    let mut index = repo.index()?;
    index.add_path(Path::new("README.md"))?;
    let tree_id = index.write_tree()?;
    index.write()?;

    // Create tree and commit
    let tree = repo.find_tree(tree_id)?;
    let sig = git2::Signature::now("test", "test@example.com")?;

    // Create main branch commit, no parents
    let oid = repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])?;

    // Set HEAD to main and checkout
    repo.set_head("refs/heads/main")?;
    let obj = repo.find_object(oid, None)?;
    repo.checkout_tree(&obj, None)?;
    Ok(())
  }
}
