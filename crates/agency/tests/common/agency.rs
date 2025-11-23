use crate::common::test_env::TestEnv;
use anyhow::{Context, Result, anyhow};
use gix as git;
use regex::Regex;

impl TestEnv {
  pub fn bootstrap_task(&self, id: u32) -> Result<()> {
    self
      .agency()?
      .arg("bootstrap")
      .arg(id.to_string())
      .assert()
      .success();
    Ok(())
  }

  pub fn agency_daemon_start(&self) -> Result<()> {
    self.agency()?.arg("daemon").arg("start").assert().success();
    Ok(())
  }

  pub fn agency_daemon_stop(&self) -> Result<()> {
    self.agency()?.arg("daemon").arg("stop").assert().success();
    Ok(())
  }

  pub fn new_task(&self, slug: &str, extra_args: &[&str]) -> Result<(u32, String)> {
    let agen_dir = self.path().join(".agency");
    let cfg_path = agen_dir.join("agency.toml");
    if !cfg_path.exists() {
      let _ = std::fs::create_dir_all(&agen_dir);
      let cfg = "[agents.sh]\ncmd = [\"sh\"]\n";
      std::fs::write(&cfg_path, cfg).context("write test agent config")?;
    }

    let mut cmd = self.agency()?;
    cmd.arg("new");
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

    let stdout = String::from_utf8_lossy(&out.stdout);
    let re_new = Regex::new(r"(?i)Create task ([A-Za-z][A-Za-z0-9-]*) \(id (\d+)\)")
      .expect("regex new");
    let caps = re_new
      .captures(&stdout)
      .with_context(|| format!("unexpected stdout: {stdout}"))?;
    let final_slug = caps.get(1).unwrap().as_str().to_string();
    let id: u32 = caps.get(2).unwrap().as_str().parse().context("id parse")?;
    Ok((id, final_slug))
  }

  pub fn task_file_path(&self, id: u32, slug: &str) -> std::path::PathBuf {
    self
      .path()
      .join(".agency")
      .join("tasks")
      .join(format!("{id}-{slug}.md"))
  }

  pub fn worktree_dir_path(&self, id: u32, slug: &str) -> std::path::PathBuf {
    self
      .path()
      .join(".agency")
      .join("worktrees")
      .join(format!("{id}-{slug}"))
  }

  #[allow(clippy::unused_self)]
  pub fn branch_name(&self, id: u32, slug: &str) -> String {
    format!("agency/{id}-{slug}")
  }

  pub fn branch_exists(&self, id: u32, slug: &str) -> Result<bool> {
    let repo = git::discover(self.path())?;
    let full = format!("refs/heads/{}", self.branch_name(id, slug));
    Ok(repo.find_reference(&full).is_ok())
  }

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
}
