#![allow(dead_code)]
use anyhow::{Context, Result, anyhow};
use assert_cmd::Command;

use gix as git;
#[cfg(unix)]
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use temp_env::with_vars;
use tempfile::{Builder, TempDir};

#[derive(Debug)]
pub struct TestEnv {
  temp: TempDir,
  runtime_dir: PathBuf,
  xdg_home: PathBuf,
}

impl TestEnv {
  /// Run a test closure inside a fresh `TestEnv`.
  pub fn run<F, R>(f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    let env = TestEnv::new();
    let current_path = std::env::var("PATH").ok();
    let bin_dir = env.xdg_home_bin_dir();
    let path_value = match current_path {
      Some(existing) if !existing.is_empty() => {
        format!("{}:{existing}", bin_dir.display())
      }
      _ => bin_dir.display().to_string(),
    };
    let runtime_dir = env.runtime_dir().to_path_buf();
    with_vars(
      [
        (
          "XDG_CONFIG_HOME",
          Some(env.xdg_home_dir().display().to_string()),
        ),
        ("PATH", Some(path_value)),
        ("XDG_RUNTIME_DIR", Some(runtime_dir.display().to_string())),
        ("EDITOR", Some("bash -lc true".to_string())),
        ("AGENCY_NO_AUTOSTART", Some("1".to_string())),
      ],
      || f(&env),
    )
  }

  pub fn new() -> Self {
    let root = tmp_root();
    let temp = Builder::new()
      .prefix("agency-test-")
      .tempdir_in(root)
      .expect("temp dir");
    // Provide a local agent configuration for tests using a simple shell.
    // This ensures `-a sh` is valid without relying on embedded defaults.
    let workdir = temp.path();
    let agen_dir = workdir.join(".agency");
    let _ = std::fs::create_dir_all(&agen_dir);
    let cfg_path = agen_dir.join("agency.toml");
    let cfg = "[agents.sh]\ncmd = [\"sh\"]\n";
    if let Err(err) = std::fs::write(&cfg_path, cfg) {
      panic!("write test agent config failed: {err}");
    }

    // Create a unique, short runtime dir per test to isolate daemon sockets
    let runtime_dir = runtime_dir_create();

    // Create a per-test XDG config home under the sandbox root
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_nanos())
      .unwrap_or(0);
    let xdg_home = tmp_root().join(format!("xdg-{nanos}"));
    let _ = std::fs::create_dir_all(&xdg_home);

    Self {
      temp,
      runtime_dir,
      xdg_home,
    }
  }

  /// Temporarily set environment variables for the duration of `f`.
  ///
  /// Restores previous values after the closure returns or panics.
  pub fn with_env_vars<F, R>(&self, vars: &[(&str, Option<String>)], f: F) -> R
  where
    F: FnOnce(&TestEnv) -> R,
  {
    with_vars(vars, || f(self))
  }

  /// Prepare a task's branch/worktree and run bootstrap (no PTY attach).
  pub fn bootstrap_task(&self, id: u32) -> Result<()> {
    self
      .agency()?
      .arg("bootstrap")
      .arg(id.to_string())
      .assert()
      .success();
    Ok(())
  }

  /// Start the agency daemon for this test environment.
  ///
  /// # Errors
  ///
  /// Returns an error if the command cannot be spawned.
  pub fn agency_daemon_start(&self) -> Result<()> {
    self.agency()?.arg("daemon").arg("start").assert().success();
    Ok(())
  }

  /// Stop the agency daemon for this test environment.
  ///
  /// # Errors
  ///
  /// Returns an error if the command cannot be spawned.
  pub fn agency_daemon_stop(&self) -> Result<()> {
    self.agency()?.arg("daemon").arg("stop").assert().success();
    Ok(())
  }

  pub fn path(&self) -> &std::path::Path {
    self.temp.path()
  }

  pub fn runtime_dir(&self) -> &std::path::Path {
    &self.runtime_dir
  }

  /// XDG config home directory for this test environment.
  pub fn xdg_home_dir(&self) -> &std::path::Path {
    &self.xdg_home
  }

  /// `bin` directory inside the XDG config home.
  pub fn xdg_home_bin_dir(&self) -> std::path::PathBuf {
    self.xdg_home_dir().join("bin")
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

  /// Construct a command for invoking the `agency` binary in this test env.
  ///
  /// # Errors
  ///
  /// Returns an error if the binary cannot be located.
  pub fn agency(&self) -> Result<Command> {
    let mut cmd = Command::cargo_bin("agency")?;
    // Ensure all test-launched binaries use a per-test XDG runtime dir
    // so the daemon socket is created inside the sandbox workspace.
    cmd.current_dir(self.path());
    cmd.env("XDG_RUNTIME_DIR", &self.runtime_dir);
    // Ensure sockets are isolated per test via explicit env overrides
    let uds_path = self.runtime_dir.join("agency.sock");
    let tmux_sock_path = self.runtime_dir.join("agency-tmux.sock");
    cmd.env("AGENCY_SOCKET_PATH", &uds_path);
    cmd.env("AGENCY_TMUX_SOCKET_PATH", &tmux_sock_path);
    // Hint CLI to treat STDIN/STDOUT as non-TTY for wizard fallbacks
    cmd.env("AGENCY_TEST", "1");
    Ok(cmd)
  }

  fn git(&self) -> std::process::Command {
    // Use the standard `git` CLI for repository operations in tests.
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(self.path());
    cmd
  }

  /// Run `git` with the given arguments and return trimmed UTF-8 stdout.
  ///
  /// # Errors
  ///
  /// Returns an error if the process cannot be spawned, exits unsuccessfully,
  /// or stdout is not valid UTF-8.
  pub fn git_stdout(&self, args: &[&str]) -> Result<String> {
    let output = self
      .git()
      .args(args)
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::inherit())
      .output()
      .context("run git command")?;
    if !output.status.success() {
      return Err(anyhow!(
        "git {:?} failed with status {status} and stderr: {stderr}",
        args,
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
      ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
  }

  /// Return the current `HEAD` commit id as a hex string.
  ///
  /// # Errors
  ///
  /// Returns an error if `git rev-parse HEAD` fails.
  pub fn git_head_hex(&self) -> Result<String> {
    self.git_stdout(&["rev-parse", "HEAD"])
  }

  /// Create a new `agency/<id>-<slug>` branch at `HEAD`.
  ///
  /// # Errors
  ///
  /// Returns an error if `git branch` fails.
  pub fn git_new_branch(&self, id: u32, slug: &str) -> Result<()> {
    let branch = self.branch_name(id, slug);
    let status = self
      .git()
      .arg("branch")
      .arg(&branch)
      .arg("HEAD")
      .status()
      .context("run git branch")?;
    if !status.success() {
      return Err(anyhow!(
        "git branch {branch} HEAD failed with status {status}"
      ));
    }
    Ok(())
  }

  /// Create a new worktree for `agency/<id>-<slug>` at `HEAD`.
  ///
  /// This creates the branch and its worktree in a single command.
  ///
  /// # Errors
  ///
  /// Returns an error if `git worktree add` fails.
  pub fn git_add_worktree(&self, id: u32, slug: &str) -> Result<()> {
    let branch = self.branch_name(id, slug);
    let dir = self.worktree_dir_path(id, slug);
    let status = self
      .git()
      .arg("worktree")
      .arg("add")
      .arg("--quiet")
      .arg(&dir)
      .arg("-b")
      .arg(&branch)
      .arg("HEAD")
      .status()
      .context("run git worktree add")?;
    if !status.success() {
      return Err(anyhow!(
        "git worktree add for {branch} at {} failed with status {status}",
        dir.display(),
        status = status
      ));
    }
    Ok(())
  }

  /// Return the head commit id for the given branch.
  ///
  /// # Errors
  ///
  /// Returns an error if the repository cannot be opened or the reference
  /// cannot be resolved to an object id.
  pub fn git_branch_head_id(&self, branch: &str) -> Result<git::ObjectId> {
    let repo = git::discover(self.path()).context("open git repo in TestEnv")?;
    let full_ref = format!("refs/heads/{branch}");
    let reference = repo
      .find_reference(&full_ref)
      .with_context(|| format!("find reference {full_ref}"))?;
    let target = reference.target();
    let id = target.try_id().context("reference target is not an id")?;
    Ok(id.into())
  }

  /// Create an empty-tree commit on the task branch `agency/<id>-<slug>`.
  ///
  /// This uses `gix` to write directly to the task branch without modifying
  /// the working tree.
  ///
  /// # Errors
  ///
  /// Returns an error if the repository cannot be opened or the commit fails.
  pub fn git_commit_empty_tree_to_task_branch(
    &self,
    id: u32,
    slug: &str,
    message: &str,
  ) -> Result<git::ObjectId> {
    let repo = git::discover(self.path()).context("open git repo in TestEnv")?;
    let empty_tree = git::ObjectId::empty_tree(repo.object_hash());
    let head = repo.head_commit().context("resolve HEAD commit")?;
    let parent_id = head.id();
    let task_ref = format!("refs/heads/{}", self.branch_name(id, slug));
    let new_id = repo
      .commit(task_ref.as_str(), message, empty_tree, [parent_id])
      .context("create empty-tree commit on task branch")?;
    Ok(new_id.into())
  }

  /// Return the porcelain status (without untracked files) of the repo.
  ///
  /// # Errors
  ///
  /// Returns an error if `git status` fails.
  pub fn git_status_porcelain(&self) -> Result<String> {
    self.git_stdout(&["status", "--porcelain", "--untracked-files=no"])
  }

  /// Add all changes and create a commit with the given message.
  ///
  /// # Errors
  ///
  /// Returns an error if either `git add -A` or `git commit` fails.
  pub fn git_add_all_and_commit(&self, message: &str) -> Result<()> {
    let _ = self.git_stdout(&["add", "-A"])?;
    let _ = self.git_stdout(&["commit", "-m", message])?;
    Ok(())
  }

  /// Return the `git stash list` output.
  ///
  /// # Errors
  ///
  /// Returns an error if `git stash list` fails.
  pub fn git_stash_list(&self) -> Result<String> {
    self.git_stdout(&["stash", "list"])
  }

  /// Check whether the worktree directory for the given task exists.
  pub fn git_worktree_exists(&self, id: u32, slug: &str) -> bool {
    self.worktree_dir_path(id, slug).is_dir()
  }

  /// Run `agency gc` and return the captured output.
  ///
  /// # Errors
  ///
  /// Returns an error if the command fails to spawn or exits unsuccessfully.
  pub fn agency_gc(&self) -> Result<std::process::Output> {
    let output = self.agency()?.arg("gc").output().context("run agency gc")?;
    if !output.status.success() {
      return Err(anyhow!(
        "agency gc failed with status {status} and stderr: {stderr}",
        status = output.status,
        stderr = String::from_utf8_lossy(&output.stderr)
      ));
    }
    Ok(output)
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
    // Ensure a local agent configuration is present for tests that specify `-a sh`
    // without relying on embedded defaults.
    let agen_dir = self.path().join(".agency");
    let cfg_path = agen_dir.join("agency.toml");
    if !cfg_path.exists() {
      let _ = std::fs::create_dir_all(&agen_dir);
      let cfg = "[agents.sh]\ncmd = [\"sh\"]\n";
      std::fs::write(&cfg_path, cfg).context("write test agent config")?;
    }

    let mut cmd = self.agency()?;
    cmd.arg("new");
    // Default to draft mode in tests unless explicitly overridden
    let mut has_draft = false;
    let mut has_description = false;
    for arg_value in extra_args {
      if *arg_value == "--draft" {
        has_draft = true;
      }
      if *arg_value == "--description" {
        has_description = true;
      }
      cmd.arg(arg_value);
    }
    if !has_draft {
      cmd.arg("--draft");
    }
    // Ensure non-interactive by default with a fixed description
    if !has_description {
      cmd.arg("--description").arg("Automated test");
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
    let re_new = regex::Regex::new(r"(?i)Create task ([A-Za-z][A-Za-z0-9-]*) \(id (\d+)\)")?;
    let caps = re_new
      .captures(&stdout)
      .with_context(|| format!("unexpected stdout: {stdout}"))?;
    let final_slug = caps
      .get(1)
      .context("missing slug capture")?
      .as_str()
      .to_string();
    let id: u32 = caps
      .get(2)
      .context("missing id capture")?
      .as_str()
      .parse()
      .context("id parse")?;
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

  /// Write an executable script with the given body.
  ///
  /// # Errors
  ///
  /// Returns an error if the parent directory cannot be created, the file
  /// cannot be written, or its permissions cannot be updated.
  pub fn write_executable_script(&self, path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).with_context(|| {
        format!(
          "create parent dir for script under {}",
          self.path().display()
        )
      })?;
    }
    std::fs::write(path, body).with_context(|| {
      format!(
        "write script body at {} relative to {}",
        path.display(),
        self.path().display()
      )
    })?;
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt as _;
      let mut perms = std::fs::metadata(path)
        .with_context(|| format!("read script metadata at {}", path.display()))?
        .permissions();
      perms.set_mode(0o755);
      std::fs::set_permissions(path, perms)
        .with_context(|| format!("set script executable at {}", path.display()))?;
    }
    Ok(())
  }

  /// Write a UTF-8 file at a path relative to the test root.
  ///
  /// Returns the absolute path to the written file.
  ///
  /// # Errors
  ///
  /// Returns an error if the parent directory cannot be created or the file
  /// cannot be written.
  pub fn write_file(&self, relative: &str, body: &str) -> Result<std::path::PathBuf> {
    let path = self.path().join(relative);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .with_context(|| format!("create parent dir for file under {}", self.path().display()))?;
    }
    std::fs::write(&path, body).with_context(|| {
      format!(
        "write file body at {} relative to {}",
        path.display(),
        self.path().display()
      )
    })?;
    Ok(path)
  }

  /// Add an executable script under the XDG home `bin` directory.
  ///
  /// # Errors
  ///
  /// Returns an error if the script cannot be created or made executable.
  pub fn add_xdg_home_bin(&self, name: &str, body: &str) -> Result<std::path::PathBuf> {
    let bin_dir = self.xdg_home_bin_dir();
    let script_path = bin_dir.join(name);
    self.write_executable_script(&script_path, body)?;
    Ok(script_path)
  }

  /// Write a UTF-8 file under the XDG config home.
  ///
  /// Returns the absolute path to the written file.
  ///
  /// # Errors
  ///
  /// Returns an error if the parent directory cannot be created or the file
  /// cannot be written.
  pub fn write_xdg_config(&self, relative: &str, body: &str) -> Result<std::path::PathBuf> {
    let path = self.xdg_home_dir().join(relative);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).with_context(|| {
        format!(
          "create parent dir for XDG config under {}",
          self.xdg_home_dir().display()
        )
      })?;
    }
    std::fs::write(&path, body).with_context(|| {
      format!(
        "write XDG config at {} under {}",
        path.display(),
        self.xdg_home_dir().display()
      )
    })?;
    Ok(path)
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

// No external agent preparation needed for tests; we use `/bin/sh` via [agents.sh].

impl Drop for TestEnv {
  fn drop(&mut self) {
    // Best-effort: kill the tmux server bound to this test's socket
    // Use the same logic as when spawning commands: <runtime_dir>/agency-tmux.sock
    let tmux_sock = self.runtime_dir.join("agency-tmux.sock");
    let _ = std::process::Command::new("tmux")
      .arg("-S")
      .arg(tmux_sock)
      .arg("kill-server")
      .status();

    // Best-effort: stop daemon for this test's runtime dir
    // Do not panic in Drop; ignore all errors
    if let Ok(mut cmd) = Command::cargo_bin("agency") {
      let _ = cmd
        .current_dir(self.path())
        .arg("daemon")
        .arg("stop")
        .output();
    }
    // Best-effort: remove the per-test runtime dir (sockets, etc.)
    let _ = std::fs::remove_dir_all(&self.runtime_dir);
  }
}
