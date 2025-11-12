use owo_colors::OwoColorize as _;

fn highlight_name() -> String {
  "Agency".bright_cyan().bold().to_string()
}

fn highlight_cmd(cmd: &str) -> String {
  format!("{}", cmd.bright_cyan().bold())
}

pub fn welcome_lines(config_path: &str) -> Vec<String> {
  let path = format!("{}", config_path.bright_cyan());
  vec![
    format!(
      "Welcome to {}! The home for your CLI agents.",
      highlight_name()
    ),
    String::new(),
    "Let's get you started as quickly as possible. Just choose a few default options.".to_string(),
    String::new(),
    format!("You can always tweak these later in {}.", path),
  ]
}

pub fn agent_prompt() -> String {
  "Select the agent that you want to use by default:".to_string()
}

pub fn agent_warning_when_missing() -> String {
  "No configured agent executables were detected in PATH. You can still choose one, but you may need to install it afterwards."
    .bright_yellow()
    .to_string()
}

pub fn detach_prompt() -> String {
  format!(
    "Which shortcut do you want to use to {} from agents?",
    "detach".bright_cyan()
  )
}

pub fn shell_prompt() -> String {
  "Which shell should Agency use when opening a shell? (enter a command)".to_string()
}

pub fn summary_lines() -> Vec<String> {
  vec![
    String::new(),
    "What's next?".bright_cyan().bold().to_string(),
    String::new(),
    format!(
      "- Explore every built-in option via {}",
      highlight_cmd("agency defaults")
    ),
    format!(
      "- Scaffold local overrides with {}",
      highlight_cmd("agency init")
    ),
    format!("- Need a refresher? Run {}", highlight_cmd("agency --help")),
    String::new(),
    "You are all set up!".bright_green().bold().to_string(),
    String::new(),
    format!(
      "Just run {} in your codebase and press {} to start your first agent.",
      highlight_cmd("agency"),
      "N".bright_magenta().bold()
    ),
  ]
}
