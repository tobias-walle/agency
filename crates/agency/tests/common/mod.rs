use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
}

impl TestEnv {
  pub fn new() -> Self {
    let temp = TempDir::new().expect("temp dir");
    // Ensure the fake agent script is available relative to the temp workdir
    // so the daemon (which uses relative ./scripts/fake_agent.py) can start.
    let workdir = temp.path();
    let scripts_dir = workdir.join("scripts");
    let _ = std::fs::create_dir_all(&scripts_dir);
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fake_agent.py");
    let dst = scripts_dir.join("fake_agent.py");
    if let Err(err) = fs_extra::file::copy(&src, &dst, &fs_extra::file::CopyOptions::new()) {
      panic!(
        "failed to copy fake agent from {} to {}: {}",
        src.display(),
        dst.display(),
        err
      );
    }
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      if let Ok(meta) = std::fs::metadata(&dst) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&dst, perms);
      }
    }

    Self { temp }
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
