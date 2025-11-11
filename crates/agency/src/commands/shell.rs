use std::process::Command as ProcCommand;

use anyhow::{Context, Result, bail};

use crate::config::{AgencyConfig, AppContext};
use crate::log_info;
use crate::utils::interactive;
use crate::utils::log::t;
use crate::utils::task::{resolve_id_or_slug, worktree_dir};

fn resolve_shell_argv(cfg: &AgencyConfig) -> Vec<String> {
  if let Some(v) = &cfg.shell {
    if !v.is_empty() {
      return v.clone();
    }
  }
  if let Ok(sh) = std::env::var("SHELL") {
    if !sh.trim().is_empty() {
      return vec![sh];
    }
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
  let program = argv.get(0).map(|s| s.trim()).unwrap_or("");
  if program.is_empty() {
    bail!("shell program is empty");
  }
  let args: Vec<&str> = argv.iter().skip(1).map(|s| s.as_str()).collect();

  log_info!("Open shell {}", t::path(wt_dir.display()));

  interactive::scope(|| {
    let status = ProcCommand::new(program)
      .args(&args)
      .current_dir(&wt_dir)
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
      let mut cfg = AgencyConfig::default();
      cfg.shell = Some(vec!["zsh".to_string(), "-l".to_string()]);
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
