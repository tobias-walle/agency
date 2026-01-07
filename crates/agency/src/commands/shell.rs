use std::process::Command as ProcCommand;

use anyhow::{Context, Result, bail};

use crate::config::AppContext;
use crate::log_info;
use crate::utils::files::has_files;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::log::t;
use crate::utils::session::build_task_env;
use crate::utils::shell::resolve_shell_argv;
use crate::utils::task::{read_task_content, resolve_id_or_slug, worktree_dir};

pub fn run(ctx: &AppContext, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(&ctx.paths, ident)?;
  let wt_dir = worktree_dir(&ctx.paths, &tref);
  if !wt_dir.exists() {
    bail!(
      "worktree not found at {}. Run `agency bootstrap {}` or `agency start {}` first",
      wt_dir.display(),
      tref.id,
      tref.id
    );
  }

  let shell_argv = resolve_shell_argv(&ctx.config);
  let shell_program = shell_argv.first().map_or("", |s| s.trim());
  if shell_program.is_empty() {
    bail!("shell program is empty");
  }
  let shell_args: Vec<&str> = shell_argv
    .iter()
    .skip(1)
    .map(std::string::String::as_str)
    .collect();

  // Build environment variables
  let content = read_task_content(&ctx.paths, &tref)?;
  let description = content.body.trim();
  let repo = open_main_repo(ctx.paths.root())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.root());
  let task_has_files = has_files(&ctx.paths, &tref);
  let env_map = build_task_env(tref.id, description, &repo_root, task_has_files);

  log_info!("Open shell {}", t::path(wt_dir.display()));

  interactive::scope(|| {
    let status = ProcCommand::new(shell_program)
      .args(&shell_args)
      .current_dir(&wt_dir)
      .envs(&env_map)
      .status()
      .with_context(|| format!("failed to spawn shell program: {shell_program}"))?;
    if !status.success() {
      bail!("shell exited with non-zero status");
    }
    Ok(())
  })
}
