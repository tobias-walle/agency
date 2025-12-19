use std::process::Command as ProcCommand;

use anyhow::{Context, Result, bail};

use crate::config::{AgencyConfig, AppContext};
use crate::log_info;
use crate::utils::git::{open_main_repo, repo_workdir_or};
use crate::utils::interactive;
use crate::utils::log::t;
use crate::utils::session::build_task_env;
use crate::utils::task::{read_task_content, resolve_id_or_slug, worktree_dir};

pub(crate) fn resolve_shell_argv(cfg: &AgencyConfig) -> Vec<String> {
  if let Some(v) = &cfg.shell
    && !v.is_empty()
  {
    return v.clone();
  }
  if let Ok(sh) = std::env::var("SHELL")
    && !sh.trim().is_empty()
  {
    return vec![sh];
  }
  vec!["/bin/sh".to_string()]
}

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

  let argv = resolve_shell_argv(&ctx.config);
  let program = argv.first().map_or("", |s| s.trim());
  if program.is_empty() {
    bail!("shell program is empty");
  }
  let argv_tail: Vec<&str> = argv
    .iter()
    .skip(1)
    .map(std::string::String::as_str)
    .collect();

  // Build environment variables
  let content = read_task_content(&ctx.paths, &tref)?;
  let description = content.body.trim();
  let repo = open_main_repo(ctx.paths.cwd())?;
  let repo_root = repo_workdir_or(&repo, ctx.paths.cwd());
  let env_map = build_task_env(tref.id, description, &repo_root);

  log_info!("Open shell {}", t::path(wt_dir.display()));

  interactive::scope(|| {
    let status = ProcCommand::new(program)
      .args(&argv_tail)
      .current_dir(&wt_dir)
      .envs(&env_map)
      .status()
      .with_context(|| format!("failed to spawn shell program: {program}"))?;
    if !status.success() {
      bail!("shell exited with non-zero status");
    }
    Ok(())
  })
}

#[cfg(test)]
mod tests {
  use super::resolve_shell_argv;
  use crate::config::AgencyConfig;
  use temp_env::with_vars;

  #[test]
  fn prefers_config_over_env() {
    with_vars([("SHELL", Some("/bin/bash"))], || {
      let cfg = AgencyConfig {
        shell: Some(vec!["zsh".to_string(), "-l".to_string()]),
        ..Default::default()
      };
      let argv = resolve_shell_argv(&cfg);
      assert_eq!(argv, vec!["zsh", "-l"]);
    });
  }

  #[test]
  fn falls_back_to_env_shell() {
    with_vars([("SHELL", Some("/bin/fish"))], || {
      let cfg = AgencyConfig::default();
      let argv = resolve_shell_argv(&cfg);
      assert_eq!(argv, vec!["/bin/fish"]);
    });
  }

  #[test]
  fn falls_back_to_bin_sh_when_env_missing() {
    with_vars([("SHELL", Option::<&str>::None)], || {
      let cfg = AgencyConfig::default();
      let argv = resolve_shell_argv(&cfg);
      assert_eq!(argv, vec!["/bin/sh"]);
    });
  }
}
