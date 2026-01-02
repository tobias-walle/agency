use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::config::AppContext;
use crate::log_info;
use crate::utils::wizard::{Choice, Wizard};

const SKILL_CONTENT: &str =
  include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../skills/agency/SKILL.md"));

/// Install the embedded Agency skill file into a target skills directory.
///
/// # Errors
/// Returns an error if TTY is not interactive, the home directory cannot be
/// resolved for `~` expansion, directories cannot be created, or the skill
/// file cannot be written.
pub fn run(ctx: &AppContext) -> Result<()> {
  ctx.tty.require_interactive()?;

  let wizard = Wizard::new();
  let options = vec![
    Choice {
      value: "claude".to_string(),
      label: "Claude (~/.claude/skills)".to_string(),
      detail: Some("Install under ~/.claude/skills/agency/SKILL.md".to_string()),
    },
    Choice {
      value: "codex".to_string(),
      label: "Codex (~/.codex/skills)".to_string(),
      detail: Some("Install under ~/.codex/skills/agency/SKILL.md".to_string()),
    },
    Choice {
      value: "custom".to_string(),
      label: "Custom path".to_string(),
      detail: Some("Install into a custom directory".to_string()),
    },
  ];

  let choice = wizard.select(
    "Install Agency skill for which tool?",
    &options,
    Some("codex"),
  )?;

  let custom_dir = if choice == "custom" {
    Some(wizard.text(
      "Enter directory to install Agency skill (e.g., ~/.config/agency-skills)",
      "~/.codex/skills",
    )?)
  } else {
    None
  };

  let target_path = resolve_target_path(&choice, custom_dir.as_deref())?;
  if let Some(parent) = target_path.parent() {
    fs::create_dir_all(parent)
      .with_context(|| format!("failed to create {}", parent.display()))?;
  }

  if target_path.exists() {
    let overwrite = ctx.tty.confirm(
      &format!(
        "Skill file already exists at {}. Overwrite?",
        target_path.display()
      ),
      false,
      false,
    )?;
    if !overwrite {
      log_info!("Skipping installation; existing skill file left untouched.");
      return Ok(());
    }
  }

  fs::write(&target_path, SKILL_CONTENT)
    .with_context(|| format!("failed to write skill file to {}", target_path.display()))?;
  log_info!("Installed Agency skill to {}", target_path.display());
  Ok(())
}

/// Resolve the final SKILL.md path for a given choice and optional custom input.
///
/// # Errors
/// Returns an error if the home directory cannot be resolved when expanding `~`
/// or if the choice is unknown.
fn resolve_target_path(choice: &str, custom_dir: Option<&str>) -> Result<PathBuf> {
  let base_dir = match choice {
    "claude" => expand_tilde("~/.claude/skills")?,
    "codex" => expand_tilde("~/.codex/skills")?,
    "custom" => {
      let Some(raw) = custom_dir else {
        anyhow::bail!("custom directory is required for custom install");
      };
      expand_tilde(raw)?
    }
    other => anyhow::bail!("unknown install choice: {other}"),
  };
  Ok(base_dir.join("agency").join("SKILL.md"))
}

/// Expand a path string that may start with `~` into an absolute path.
///
/// # Errors
/// Returns an error if the home directory cannot be resolved when `~` expansion
/// is required.
fn expand_tilde(input: &str) -> Result<PathBuf> {
  if input == "~" {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to resolve home directory"))?;
    return Ok(home);
  }
  if let Some(stripped) = input.strip_prefix("~/") {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("unable to resolve home directory"))?;
    return Ok(home.join(stripped));
  }
  Ok(Path::new(input).to_path_buf())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn expand_tilde_uses_home_for_tilde() {
    let home = dirs::home_dir().expect("home dir must exist for test");
    assert_eq!(expand_tilde("~").unwrap(), home);
    assert_eq!(expand_tilde("~/foo").unwrap(), home.join("foo"));
  }

  #[test]
  fn expand_tilde_leaves_non_tilde_paths_untouched() {
    assert_eq!(
      expand_tilde("/tmp/skills").unwrap(),
      PathBuf::from("/tmp/skills")
    );
    assert_eq!(
      expand_tilde("relative/path").unwrap(),
      PathBuf::from("relative/path")
    );
  }

  #[test]
  fn resolve_target_path_builds_expected_suffixes() {
    let claude = resolve_target_path("claude", None).unwrap();
    assert!(
      claude.ends_with(Path::new("agency").join("SKILL.md")),
      "claude path must end with agency/SKILL.md"
    );

    let codex = resolve_target_path("codex", None).unwrap();
    assert!(
      codex.ends_with(Path::new("agency").join("SKILL.md")),
      "codex path must end with agency/SKILL.md"
    );

    let custom = resolve_target_path("custom", Some("/custom/skills")).unwrap();
    assert_eq!(
      custom,
      PathBuf::from("/custom/skills").join("agency").join("SKILL.md")
    );
  }
}
