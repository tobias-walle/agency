use std::collections::BTreeMap;
use std::fs;

use anyhow::{Context, Result};
use toml::value::Table as TomlTable;

use crate::config::{self, AgencyConfig, AppContext};
use crate::log_success;
use crate::log_warn;
use crate::texts;
use crate::utils::which;
use crate::utils::wizard::{Choice, Wizard};

pub fn run(ctx: &AppContext) -> Result<()> {
  let mut global = load_global_config()?;
  let existing_agent = global.file.agent.clone();

  let wizard = Wizard::new();
  anstream::println!();
  anstream::println!();
  Wizard::print_logo();
  anstream::println!();
  anstream::println!();
  let welcome = texts::setup::welcome_lines(&global.display);
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
  let shell_defaults = shell_defaults(&ctx.config);
  let shell_prompt = texts::setup::shell_prompt();
  let shell_argv = wizard.shell_words(&shell_prompt, &shell_defaults.prompt_default)?;
  anstream::println!();

  // Ask for preferred editor command (argv split via shell-words)
  let editor_prompt = texts::setup::editor_prompt();
  let editor_defaults = editor_defaults(&ctx.config);
  let editor_argv = wizard.shell_words(&editor_prompt, &editor_defaults.prompt_default)?;
  anstream::println!();

  apply_agent_choice(&mut global.file, &default_agent);
  apply_shell_choice(&mut global.file, &shell_defaults, &shell_argv);
  apply_editor_choice(&mut global.file, &editor_defaults, &editor_argv);
  write_global_config(&global)?;

  if global.existed {
    log_warn!("Updated existing config {}", global.path.display());
  } else {
    log_success!("Created global config {}", global.path.display());
  }

  let summary = texts::setup::summary_lines();
  Wizard::info_lines(&summary);
  Ok(())
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct GlobalConfigFile {
  #[serde(default)]
  agent: Option<String>,
  #[serde(default)]
  shell: Option<Vec<String>>,
  #[serde(default)]
  editor: Option<Vec<String>>,
  #[serde(flatten)]
  extra: TomlTable,
}

#[derive(Debug)]
struct GlobalConfigState {
  path: std::path::PathBuf,
  existed: bool,
  display: String,
  file: GlobalConfigFile,
}

fn load_global_config() -> Result<GlobalConfigState> {
  let path = config::global_config_path()?;
  let existed = path.exists();
  let display = path.display().to_string();

  let file = if existed {
    let raw = fs::read_to_string(&path)
      .with_context(|| format!("failed to read {}", path.display()))?;
    if raw.trim().is_empty() {
      GlobalConfigFile::default()
    } else {
      toml::from_str::<GlobalConfigFile>(&raw)
        .with_context(|| format!("invalid TOML in {}", path.display()))?
    }
  } else {
    GlobalConfigFile::default()
  };

  Ok(GlobalConfigState {
    path,
    existed,
    display,
    file,
  })
}

fn write_global_config(state: &GlobalConfigState) -> Result<()> {
  if let Some(parent) = state.path.parent() {
    fs::create_dir_all(parent)
      .with_context(|| format!("failed to create {}", parent.display()))?;
  }

  let serialized =
    toml::to_string_pretty(&state.file).context("failed to serialize config")?;

  // For new config files, append the commented template for discoverability
  let content = if state.existed {
    serialized
  } else {
    format!("{}\n{}", serialized.trim_end(), config::config_template())
  };

  fs::write(&state.path, content)
    .with_context(|| format!("failed to write {}", state.path.display()))?;
  Ok(())
}

#[derive(Debug)]
struct ShellDefaults {
  env: Option<Vec<String>>,
  prompt_default: Vec<String>,
}

fn shell_defaults(cfg: &AgencyConfig) -> ShellDefaults {
  let env_shell = std::env::var("SHELL")
    .ok()
    .and_then(|sh| {
      let trimmed = sh.trim();
      if trimmed.is_empty() {
        None
      } else {
        Some(vec![trimmed.to_string()])
      }
    });

  let prompt_default = if let Some(value) = &cfg.shell && !value.is_empty() {
    value.clone()
  } else if let Some(env) = &env_shell {
    env.clone()
  } else {
    vec!["/bin/sh".to_string()]
  };

  ShellDefaults {
    env: env_shell,
    prompt_default,
  }
}

#[derive(Debug)]
struct EditorDefaults {
  env: Option<Vec<String>>,
  prompt_default: Vec<String>,
}

fn editor_defaults(cfg: &AgencyConfig) -> EditorDefaults {
  let env_editor = config::editor_env_argv();
  let prompt_default: Vec<String> = cfg.editor_argv();

  EditorDefaults {
    env: env_editor,
    prompt_default,
  }
}

fn apply_agent_choice(file: &mut GlobalConfigFile, agent: &str) {
  file.agent = Some(agent.to_string());
}

fn apply_shell_choice(
  file: &mut GlobalConfigFile,
  defaults: &ShellDefaults,
  selected: &[String],
) {
  if let Some(env) = &defaults.env && *selected == *env {
    file.shell = None;
    return;
  }

  file.shell = Some(selected.to_vec());
}

fn apply_editor_choice(
  file: &mut GlobalConfigFile,
  defaults: &EditorDefaults,
  selected: &[String],
) {
  if let Some(env) = &defaults.env && *selected == *env {
    file.editor = None;
    return;
  }

  file.editor = Some(selected.to_vec());
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
