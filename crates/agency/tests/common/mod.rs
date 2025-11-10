#![allow(dead_code)]
use anyhow::{Context, Result};
use assert_cmd::Command;

use gix as git;
use tempfile::{Builder, TempDir};
#[cfg(unix)]
use std::os::unix::net::UnixListener;
use std::path::PathBuf;

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
  runtime_dir: PathBuf,
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
    if let Err(err) = ensure_fake_agent_at(workdir) {
      panic!("prepare fake agent failed: {}", err);
    }

    // Create a unique, short runtime dir per test to isolate daemon sockets
    let runtime_dir = runtime_dir_create();

    Self { temp, runtime_dir }
  }

  /// Prepare a task's branch/worktree and run bootstrap (no PTY attach).
  pub fn bootstrap_task(&self, id: u32) -> Result<()> {
    let mut cmd = self.bin_cmd()?;
    cmd.arg("bootstrap").arg(id.to_string());
    cmd.assert().success();
    Ok(())
  }

  pub fn path(&self) -> &std::path::Path {
    self.temp.path()
  }

  pub fn runtime_dir(&self) -> &std::path::Path {
    &self.runtime_dir
  }

  /// Best-effort check whether Unix sockets can be created in this environment.
  /// Binds a temporary socket in the test runtime dir and removes it.
  pub fn sockets_available(&self) -> bool {
    #[cfg(unix)]
    {
      let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
      let probe = self.runtime_dir.join(format!("probe-{nanos}.sock"));
      match UnixListener::bind(&probe) {
        Ok(_l) => {
          let _ = std::fs::remove_file(&probe);
          true
        }
        Err(_) => false,
      }
    }
    #[cfg(not(unix))]
    {
      false
    }
  }


  pub fn bin_cmd(&self) -> anyhow::Result<Command> {
    let mut cmd = Command::cargo_bin("agency")?;
    // Ensure all test-launched binaries use a per-test XDG runtime dir
    // so the daemon socket is created inside the sandbox workspace.
    cmd.current_dir(self.path());
    cmd.env("XDG_RUNTIME_DIR", &self.runtime_dir);
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
    for arg_value in extra_args {
      if *arg_value == "--no-attach" {
        has_no_attach = true;
      }
      cmd.arg(arg_value);
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

    // Parse only the new log format: "Create task <slug> (id <id>)"
    let stdout = String::from_utf8_lossy(&out.stdout);
    let re_new = regex::Regex::new(r"(?i)Create task ([A-Za-z][A-Za-z0-9-]*) \(id (\d+)\)")
      .expect("regex new");
    let caps = re_new
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
    // Reference self to avoid unused_self lint while keeping call sites stable
    let _ = self;
    format!("agency/{id}-{slug}")
  }

  /// Check whether branch exists in this repo.
  pub fn branch_exists(&self, id: u32, slug: &str) -> Result<bool> {
    let repo = git::discover(self.path())?;
    let full = format!("refs/heads/{}", self.branch_name(id, slug));
    Ok(repo.find_reference(&full).is_ok())
  }
}

/// Returns a workspace-local temp root for tests under `./target/test-tmp` at the workspace root.
/// Ensures the directory exists to satisfy sandboxed filesystems that forbid `/tmp`.
pub fn tmp_root() -> std::path::PathBuf {
  let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  // Walk two parents up: crates/agency -> crates -> workspace root
  let workspace_root = manifest_dir
    .parent()
    .and_then(|p| p.parent())
    .unwrap_or(&manifest_dir)
    .to_path_buf();
  let root = workspace_root.join("target").join("test-tmp");
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

/// Create (or ensure) a short runtime dir path under `target/.r/r<nanos>` and return it.
pub fn runtime_dir_create() -> std::path::PathBuf {
  // Keep path short to satisfy Unix socket path limits on macOS/BSD
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_nanos())
    .unwrap_or(0);
  let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  let workspace_root = manifest_dir
    .parent()
    .and_then(|p| p.parent())
    .unwrap_or(&manifest_dir)
    .to_path_buf();
  let runtime_base = workspace_root.join("target").join(".r");
  let _ = std::fs::create_dir_all(&runtime_base);
  let dir = runtime_base.join(format!("r{nanos}"));
  let _ = std::fs::create_dir_all(&dir);
  dir
}

/// Ensure the fake agent script exists at `<workdir>/scripts/fake_agent.py` and is executable.
pub fn ensure_fake_agent_at(workdir: &std::path::Path) -> anyhow::Result<()> {
  use std::fs;
  let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fake_agent.py");
  let scripts_dir = workdir.join("scripts");
  let _ = fs::create_dir_all(&scripts_dir);
  let dst = scripts_dir.join("fake_agent.py");
  fs_extra::file::copy(&src, &dst, &fs_extra::file::CopyOptions::new())
    .with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt as _;
    let mut perms = fs::metadata(&dst)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&dst, perms)?;
  }
  Ok(())
}
