use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use toml::Value as TomlValue;
use toml::value::Table as TomlTable;

use crate::config::{self, AppContext};
use crate::log_success;
use crate::log_warn;
use crate::texts;
use crate::utils::which;
use crate::utils::wizard::{Choice, Wizard};

pub fn run(ctx: &AppContext) -> Result<()> {
  let config_path = config::global_config_path()?;
  let config_display = config_path.display().to_string();
  let mut table = read_existing_table(&config_path)?;

  let existing_agent = table
    .get("agent")
    .and_then(|value| value.as_str())
    .map(std::string::ToString::to_string);

  let wizard = Wizard::new();
  anstream::println!();
  anstream::println!();
  Wizard::print_logo();
  anstream::println!();
  anstream::println!();
  let welcome = texts::setup::welcome_lines(&config_display);
  Wizard::info_lines(&welcome);
  anstream::println!();

  let (options, any_detected) = agent_choices(&ctx.config.agents);
  if !any_detected {
    let warning = texts::setup::agent_warning_when_missing();
    log_warn!("{}", warning);
  }
  let agent_prompt = texts::setup::agent_prompt();
  let default_agent = wizard.select(&agent_prompt, &options, existing_agent.as_deref())?;
  anstream::println!();

  // Ask for preferred shell command (argv split via shell-words)
  let shell_from_config = ctx.config.shell.clone();
  let env_shell_argv: Option<Vec<String>> = if let Ok(sh) = std::env::var("SHELL") {
    let trimmed = sh.trim();
    if trimmed.is_empty() {
      None
    } else {
      Some(vec![trimmed.to_string()])
    }
  } else {
    None
  };
  let shell_prompt = texts::setup::shell_prompt();
  // Use currently configured value (or env/SHELL, or /bin/sh) as default
  let default_shell_argv: Vec<String> =
    if let Some(value) = &shell_from_config && !value.is_empty() {
      value.clone()
    } else if let Some(env) = &env_shell_argv {
      env.clone()
    } else {
      vec!["/bin/sh".to_string()]
    };
  let shell_argv = wizard.shell_words(&shell_prompt, &default_shell_argv)?;
  anstream::println!();

  // Ask for preferred editor command (argv split via shell-words)
  let editor_prompt = texts::setup::editor_prompt();
  let env_editor_argv: Option<Vec<String>> = if let Ok(ed) = std::env::var("EDITOR") {
    let trimmed = ed.trim();
    if trimmed.is_empty() {
      None
    } else if let Ok(tokens) = shell_words::split(trimmed) {
      if tokens.is_empty() {
        None
      } else {
        Some(tokens)
      }
    } else {
      None
    }
  } else {
    None
  };
  let default_editor_argv: Vec<String> = ctx.config.editor_argv();
  let editor_argv = wizard.shell_words(&editor_prompt, &default_editor_argv)?;
  anstream::println!();

  if let Some(parent) = config_path.parent() {
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
  }

  let existed = config_path.exists();
  table.insert(
    "agent".to_string(),
    TomlValue::String(default_agent.clone()),
  );
  if let Some(env_shell) = &env_shell_argv
    && shell_argv == *env_shell
  {
    table.remove("shell");
  } else {
    table.insert(
      "shell".to_string(),
      TomlValue::Array(
        shell_argv
          .clone()
          .into_iter()
          .map(TomlValue::String)
          .collect(),
      ),
    );
  }
  if let Some(env_editor) = &env_editor_argv
    && editor_argv == *env_editor
  {
    table.remove("editor");
  } else {
    table.insert(
      "editor".to_string(),
      TomlValue::Array(
        editor_argv
          .clone()
          .into_iter()
          .map(TomlValue::String)
          .collect(),
      ),
    );
  }

  let data = TomlValue::Table(table);
  let serialized = toml::to_string_pretty(&data).context("failed to serialize config")?;
  fs::write(&config_path, serialized)
    .with_context(|| format!("failed to write {}", config_path.display()))?;

  if existed {
    log_warn!("Updated existing config {}", config_path.display());
  } else {
    log_success!("Created global config {}", config_path.display());
  }

  let summary = texts::setup::summary_lines();
  Wizard::info_lines(&summary);
  Ok(())
}

fn agent_choices(agents: &BTreeMap<String, crate::config::AgentConfig>) -> (Vec<Choice>, bool) {
  let mut choices: Vec<(bool, Choice)> = agents
    .iter()
    .map(|(name, cfg)| {
      let detected = cfg.cmd.first().and_then(|cmd| which::which(cmd)).is_some();
      let detail = if detected {
        Some("detected in PATH".to_string())
      } else {
        None
      };
      let choice = Choice {
        value: name.clone(),
        label: name.clone(),
        detail,
      };
      (detected, choice)
    })
    .collect();
  choices.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.value.cmp(&b.1.value)));
  let any_detected = choices.iter().any(|(detected, _)| *detected);
  let options = choices.into_iter().map(|(_, choice)| choice).collect();
  (options, any_detected)
}

fn read_existing_table(path: &Path) -> Result<TomlTable> {
  if !path.exists() {
    return Ok(TomlTable::new());
  }
  let raw =
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
  if raw.trim().is_empty() {
    return Ok(TomlTable::new());
  }
  match toml::from_str::<TomlValue>(&raw)? {
    TomlValue::Table(table) => Ok(table),
    _ => Err(anyhow!(
      "global config {} must be a TOML table",
      path.display()
    )),
  }
}
