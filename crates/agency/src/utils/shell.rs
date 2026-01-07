use crate::config::AgencyConfig;

/// Resolves the shell command and arguments based on configuration.
///
/// Prefers `cfg.shell` if set, then `$SHELL`, finally `/bin/sh`.
pub fn resolve_shell_argv(cfg: &AgencyConfig) -> Vec<String> {
  if let Some(configured_shell) = &cfg.shell
    && !configured_shell.is_empty()
  {
    return configured_shell.clone();
  }
  if let Ok(shell_env) = std::env::var("SHELL")
    && !shell_env.trim().is_empty()
  {
    return vec![shell_env];
  }
  vec!["/bin/sh".to_string()]
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
