use std::path::Path;

use super::paths::project_config_path;
use super::types::Config;

/// Write a default project config if it does not exist yet.
pub fn write_default_project_config(project_root: &Path) -> std::io::Result<()> {
  let path = project_config_path(project_root);
  if let Some(parent) = path.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  if !path.exists() {
    let cfg = Config::default();
    let mut s = toml::to_string_pretty(&cfg).unwrap_or_else(|_| String::from(""));
    // Ensure a [pty] section is present with a commented example for detach_keys.
    // We avoid setting a value to keep default None semantics.
    s.push_str(
      "\n# Default agent for `agency new` when --agent is not provided.\n# default_agent = \"opencode\" # or \"claude-code\" or \"fake\"\n\n# Detach key sequence for attach under the [pty] section.\n# Leave unset to use the default Ctrl-Q. Examples:\n# pty.detach_keys = \"ctrl-q\"\n# pty.detach_keys = \"ctrl-p,ctrl-q\"\n",
    );
    std::fs::write(&path, s)?;
  }
  Ok(())
}
