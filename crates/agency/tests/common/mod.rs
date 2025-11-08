#![allow(dead_code)]
use anyhow::{Context, Result};
use assert_cmd::Command;

use gix as git;
use tempfile::{Builder, TempDir};

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
}

impl TestEnv {
  pub fn new() -> Self {
    let root = tmp_root();
    let temp = Builder::new()
      .prefix("agency-test-")
      .tempdir_in(root)
      .expect("temp dir");
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
    let _ = git::init(self.path())?;
    Ok(())
  }

  pub fn simulate_initial_commit(&self) -> anyhow::Result<()> {
    // Ensure local config provides author/committer
    let cfg_path = self.path().join(".git").join("config");
    let cfg = "[user]\n\tname = test\n\temail = test@example.com\n";
    std::fs::write(&cfg_path, cfg).context("write test git config")?;
    let repo = git::open(self.path())?;
    // Create empty tree and initial commit on HEAD so HEAD is peelable
    let empty_tree_id = git::ObjectId::empty_tree(repo.object_hash());
    // Provide author/committer via environment for gix::Repository::commit()
    let _id = repo.commit(
      "HEAD",
      "init",
      empty_tree_id,
      std::iter::empty::<git::ObjectId>(),
    )?;
    Ok(())
  }

  /// Initialize an empty repo with an initial commit on `main`.
  pub fn init_repo(&self) -> anyhow::Result<()> {
    self.setup_git_repo()?;
    self.simulate_initial_commit()
  }

  /// Convenience to run `agency new [extra_args...] <slug>` and parse `(id, final_slug)`.
  pub fn new_task(&self, slug: &str, extra_args: &[&str]) -> Result<(u32, String)> {
    let mut cmd = self.bin_cmd()?;
    cmd.arg("new");
    // Default to not attaching in tests unless explicitly overridden
    let mut has_no_attach = false;
    for a in extra_args {
      if *a == "--no-attach" {
        has_no_attach = true;
      }
      cmd.arg(a);
    }
    if !has_no_attach {
      cmd.arg("--no-attach");
    }
    cmd.arg(slug);
    let out = cmd.output().context("run agency new")?;
    if !out.status.success() {
      anyhow::bail!(
        "new failed: status={} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
      );
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Parse: "Task <slug> with id <id> created"
    let re =
      regex::Regex::new(r"Task ([A-Za-z][A-Za-z0-9-]*) with id (\d+) created").expect("regex");
    let caps = re
      .captures(&stdout)
      .with_context(|| format!("unexpected stdout: {stdout}"))?;
    let final_slug = caps.get(1).unwrap().as_str().to_string();
    let id: u32 = caps.get(2).unwrap().as_str().parse().context("id parse")?;
    Ok((id, final_slug))
  }

  /// Path to `.agency/tasks/<id>-<slug>.md`.
  pub fn task_file_path(&self, id: u32, slug: &str) -> std::path::PathBuf {
    self
      .path()
      .join(".agency")
      .join("tasks")
      .join(format!("{id}-{slug}.md"))
  }

  /// Path to `.agency/worktrees/<id>-<slug>`.
  pub fn worktree_dir_path(&self, id: u32, slug: &str) -> std::path::PathBuf {
    self
      .path()
      .join(".agency")
      .join("worktrees")
      .join(format!("{id}-{slug}"))
  }

  /// Branch name `agency/<id>-<slug>`.
  pub fn branch_name(&self, id: u32, slug: &str) -> String {
    format!("agency/{id}-{slug}")
  }

  /// Check whether branch exists in this repo.
  pub fn branch_exists(&self, id: u32, slug: &str) -> Result<bool> {
    let repo = git::discover(self.path())?;
    let full = format!("refs/heads/{}", self.branch_name(id, slug));
    Ok(repo.find_reference(&full).is_ok())
  }
}

/// Returns a workspace-local temp root for tests, inside `target/test-tmp`.
/// Ensures the directory exists to satisfy sandboxed filesystems that forbid `/tmp`.
pub fn tmp_root() -> std::path::PathBuf {
  let mut root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  root.push("target");
  root.push("test-tmp");
  let _ = std::fs::create_dir_all(&root);
  root
}

/// Create a temp dir under the workspace-local temp root.
pub fn tempdir_in_sandbox() -> TempDir {
  let root = tmp_root();
  Builder::new()
    .prefix("agency-test-")
    .tempdir_in(root)
    .expect("temp dir")
}
