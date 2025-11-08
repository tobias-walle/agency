use std::collections::HashMap;

use regex::Regex;

/// Context for command expansion.
/// - `repo_root`: absolute repository root path used for `<root>` placeholder.
/// - `env`: variables used for `$VAR` expansion.
#[derive(Debug, Clone)]
pub struct CmdCtx {
  pub repo_root: String,
  pub env: HashMap<String, String>,
}

impl CmdCtx {
  pub fn with_env(repo_root: impl Into<String>, env: HashMap<String, String>) -> Self {
    Self {
      repo_root: repo_root.into(),
      env,
    }
  }

  pub fn from_process_env(repo_root: impl Into<String>) -> Self {
    let env: HashMap<String, String> = std::env::vars().collect();
    Self::with_env(repo_root, env)
  }
}

/// Expand argv tokens using context:
/// - Replace `<root>` with `ctx.repo_root`.
/// - Expand `$VARS` using `ctx.env` (unknown -> empty string).
pub fn expand_argv(argv: &[String], ctx: &CmdCtx) -> Vec<String> {
  let var_re = Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").expect("valid var regex");
  argv
    .iter()
    .map(|raw| {
      let with_root = raw.replace("<root>", &ctx.repo_root);
      var_re
        .replace_all(&with_root, |caps: &regex::Captures| {
          ctx.env.get(&caps[1]).map_or("", String::as_str)
        })
        .to_string()
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::{CmdCtx, expand_argv};
  use std::collections::HashMap;

  #[test]
  fn expands_root_placeholder() {
    let ctx = CmdCtx::with_env("/repo", HashMap::new());
    let out = expand_argv(&vec!["echo".into(), "<root>".into()], &ctx);
    assert_eq!(out, vec!["echo", "/repo"]);
  }

  #[test]
  fn expands_vars_in_tokens() {
    let mut env = HashMap::new();
    env.insert("FOO".into(), "bar".into());
    let ctx = CmdCtx::with_env("/r", env);
    let out = expand_argv(&vec!["echo".into(), "$FOO".into()], &ctx);
    assert_eq!(out, vec!["echo", "bar"]);
  }

  #[test]
  fn mixed_text_expands_inline_and_root() {
    let mut env = HashMap::new();
    env.insert("X".into(), "1".into());
    let ctx = CmdCtx::with_env("/root", env);
    let out = expand_argv(&vec!["echo".into(), "pre-<root>-$X".into()], &ctx);
    assert_eq!(out, vec!["echo", "pre-/root-1"]);
  }

  #[test]
  fn unknown_var_becomes_empty() {
    let ctx = CmdCtx::with_env("/r", HashMap::new());
    let out = expand_argv(&vec!["echo".into(), "$NOPE".into()], &ctx);
    assert_eq!(out, vec!["echo", ""]);
  }
}
