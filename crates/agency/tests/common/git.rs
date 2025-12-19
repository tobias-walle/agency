use crate::common::test_env::TestEnv;
use anyhow::{Context, Result, anyhow};
use gix as git;

impl TestEnv {
  pub fn git(&self) -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(self.path());
    cmd
  }

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

  pub fn git_head_hex(&self) -> Result<String> {
    self.git_stdout(&["rev-parse", "HEAD"])
  }

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

  pub fn git_status_porcelain(&self) -> Result<String> {
    self.git_stdout(&["status", "--porcelain", "--untracked-files=no"])
  }

  pub fn git_add_all_and_commit(&self, message: &str) -> Result<()> {
    let _ = self.git_stdout(&["add", "-A"])?;
    let _ = self.git_stdout(&["commit", "-m", message])?;
    Ok(())
  }

  pub fn git_stash_list(&self) -> Result<String> {
    self.git_stdout(&["stash", "list"])
  }

  pub fn git_worktree_exists(&self, id: u32, slug: &str) -> bool {
    self.worktree_dir_path(id, slug).is_dir()
  }

  pub fn setup_git_repo(&self) -> anyhow::Result<()> {
    let _ = git::init(self.path())?;
    Ok(())
  }

  pub fn simulate_initial_commit(&self) -> anyhow::Result<()> {
    let cfg_path = self.path().join(".git").join("config");
    let cfg = "[user]\n\tname = test\n\temail = test@example.com\n";
    std::fs::write(&cfg_path, cfg).context("write test git config")?;
    let repo = git::open(self.path())?;
    let empty_tree_id = git::ObjectId::empty_tree(repo.object_hash());
    let _id = repo.commit(
      "HEAD",
      "init",
      empty_tree_id,
      std::iter::empty::<git::ObjectId>(),
    )?;
    Ok(())
  }

  pub fn init_repo(&self) -> anyhow::Result<()> {
    self.setup_git_repo()?;
    self.simulate_initial_commit()
  }

  pub fn git_create_branch(&self, branch: &str) -> Result<()> {
    self.git_stdout(&["branch", branch])?;
    Ok(())
  }

  pub fn git_checkout(&self, branch: &str) -> Result<()> {
    self.git_stdout(&["checkout", branch])?;
    Ok(())
  }
}

