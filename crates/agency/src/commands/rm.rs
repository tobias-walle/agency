use std::fs;

use anyhow::{Context, Result};
use owo_colors::OwoColorize as _;

use crate::config::AgencyConfig;
use crate::utils::git::{open_main_repo, remove_worktree_and_branch};
use crate::utils::task::{branch_name, resolve_id_or_slug, task_file, worktree_dir, worktree_name};
use crate::utils::term::confirm;

pub fn run(cfg: &AgencyConfig, ident: &str) -> Result<()> {
  let tref = resolve_id_or_slug(cfg, ident)?;
  let branch = branch_name(&tref);
  let wt_name = worktree_name(&tref);
  let wt_dir = worktree_dir(cfg, &tref);
  let file = task_file(cfg, &tref);

  anstream::println!(
    "{}\n  file: {}\n  branch: {}\n  worktree: {}",
    "About to remove:".yellow(),
    file.display().to_string().cyan(),
    branch.cyan(),
    wt_dir.display().to_string().cyan(),
  );

  if confirm("Proceed? [y/N]")? {
    let repo = open_main_repo(cfg.cwd())?;
    remove_worktree_and_branch(&repo, &wt_name, &branch)?;
    if file.exists() {
      fs::remove_file(&file).with_context(|| format!("failed to remove {}", file.display()))?;
    }
    anstream::println!("{}", "Removed task, branch, and worktree".green());
  } else {
    anstream::println!("{}", "Cancelled".yellow());
  }

  Ok(())
}
