use std::process::Command as ProcCommand;

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::task::{read_task_content, resolve_id_or_slug, worktree_dir};

pub fn run(ctx: &AppContext, ident: &str, cmd: &[String]) -> Result<i32> {
  // Resolve task
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let wt_dir = worktree_dir(&ctx.paths, &tref);

  // Check worktree exists (mirror shell behavior)
  if !wt_dir.exists() {
    bail!(
      "worktree not found at {}. Run `agency bootstrap {}` or `agency start {}` first",
      wt_dir.display(),
      tref.id,
      tref.id
    );
  }

  // Get command parts
  let program = cmd
    .first()
    .ok_or_else(|| anyhow::anyhow!("no command provided"))?;
  let args = &cmd[1..];

  // Build environment variables
  let content = read_task_content(&ctx.paths, &tref)?;
  let description = content.body.trim();
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let root_abs = repo_root
    .canonicalize()
    .unwrap_or(repo_root)
    .display()
    .to_string();

  // Execute command (no log output from agency)
  let status = ProcCommand::new(program)
    .args(args)
    .current_dir(&wt_dir)
    .env("AGENCY_TASK", description)
    .env("AGENCY_ROOT", root_abs)
    .env("AGENCY_TASK_ID", tref.id.to_string())
    .status()
    .with_context(|| format!("failed to execute: {program}"))?;

  // Return exit code
  Ok(status.code().unwrap_or(1))
}
