# PLAN: First‑Run Setup Wizard and Support Commands

Design and integrate a friendly, colorful “first‑run” setup that creates a user config and related helper commands.

## Goals

- Add `agency setup` wizard with ASCII welcome, colors, and guided prompts.
- Auto-run setup on first start if no global XDG config exists.
- Detect installed agents from configured list; let user pick a default agent.
- Let user optionally change the default detach shortcut with guidance.
- Create or update the global config file and summarize what was written; warn if updating an existing config.
- Add `agency defaults` to print the embedded default TOML.
- Add `agency init` to scaffold `.agency/` with confirmation and summary.
- Keep copy (text + colors) easy to maintain and consistent.

## Out of scope

- PTY/TUI redesign beyond the setup flow.
- Remote or cloud configuration sync.
- Windows support.
- Agent installation or package management.

## Current Behavior

- Commands are defined and dispatched in `crates/agency/src/lib.rs:1`. No `setup`, `defaults`, or `init` exists yet.
- On no subcommand, the app starts the TUI if stdout is a TTY (`crates/agency/src/lib.rs:85`).
- Config loads by merging embedded defaults + global XDG `agency.toml` + project `.agency/agency.toml` (`crates/agency/src/config.rs:79`), never erroring if the global file is missing (defaults are always present).
- Embedded defaults live in `crates/agency/defaults/agency.toml:1` (includes default agent, keybindings, bootstrap, sample agents).
- Existing TUI is built with `ratatui` (`crates/agency/src/tui/mod.rs:1`) and logging uses project macros and `owo-colors` (`crates/agency/Cargo.toml:11`).

## Solution

- Add `setup` subcommand that:
  - Renders a small ASCII logo + welcome text with color.
  - Detects installed agents by resolving the first `cmd[0]` executable in PATH for each configured agent.
  - Presents a list of detected agents to choose default (fallback: allow choosing from all, but warn if not installed).
  - Prompts for an optional detach shortcut (pre-filled with current default).
  - Writes the global XDG config (`$XDG_CONFIG_HOME/agency/agency.toml`), merging the chosen default agent and keybinding overrides.
  - If the file already exists, update in place (merge overlays) and emit a clear warning that an existing config was modified.
  - Prints a concise, colored summary and hints (`agency defaults`, `agency init`, `agency --help`).
- Auto-run setup on first start:
  - If no subcommand and no XDG config file exists, run setup instead of launching TUI.
  - For other subcommands, do not auto-run setup; keep behavior predictable.
- Add `defaults` subcommand to print `defaults/agency.toml` verbatim, with a short header.
- Add `init` subcommand:
  - Confirm with the user.
  - Create `.agency/`, `.agency/agency.toml` (empty), `.agency/setup.sh` (executable, minimal template), and show an overview.
- Copy maintenance and colors:
  - Centralize reusable wizard UI helpers (logo, prompts, theming) in `utils/wizard.rs`.
  - Keep setup-specific strings in `texts/setup.rs` with token placeholders for easy updates.
  - Apply consistent color tokens via `owo-colors`/`anstyle` and existing log token styles.
  - Keep the ASCII logo small and in one function for readability.
- Rendering:
  - Use simple interactive prompts (non-fullscreen) via a lightweight CLI prompt crate like `inquire` (custom theme for colors) for lists/inputs and reuse log macros for messages.
  - Keep `ratatui` for the TUI; avoid full-screen for setup to minimize friction.

## Logo

```
       db
      d88b
     d8'`8b
    d8'  `8b      ,adPPYb,d8   ,adPPYba,  8b,dPPYba,    ,adPPYba,  8b       d8
   d8YaaaaY8b    a8"    `Y88  a8P_____88  88P'   `"8a  a8"     ""  `8b     d8'
  d8""""""""8b   8b       88  8PP"""""""  88       88  8b           `8b   d8'
 d8'        `8b  "8a,   ,d88  "8b,   ,aa  88       88  "8a,   ,aa    `8b,d8'
d8'          `8b  `"YbbdP"Y8   `"Ybbd8"'  88       88   `"Ybbd8"'      Y88'
                  aa,    ,88                                           d8'
                   "Y8bbdP"                                           d8'
```

(Should be rendered in cyan gradient)

## Architecture

- New files
  - `crates/agency/src/commands/setup.rs` — setup wizard flow
  - `crates/agency/src/commands/defaults.rs` — print embedded defaults
  - `crates/agency/src/commands/init.rs` — scaffold `.agency/` files
  - `crates/agency/src/utils/wizard.rs` — generic wizard helpers (logo, prompts, theme)
  - `crates/agency/src/texts/setup.rs` — setup-specific texts (strings, explanations)
  - `crates/agency/src/utils/which.rs` — simple PATH resolve/check helper
- Modified files
  - `crates/agency/src/lib.rs` — add new subcommands, and first-run gate to call setup if no XDG config and no subcommand
  - `crates/agency/src/config.rs` — add helper to get XDG config path and an `exists` check
- Dependencies
  - Add `inquire` for user prompts (theming via `cargo add inquire`)

## Detailed Plan

HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)

- [x] Tests: CLI setup flow creates global config
  - Add integration tests in `crates/agency/tests/cli.rs`:
    - Override `XDG_CONFIG_HOME` to a temp dir via `temp-env`.
    - Run `agency setup`, drive prompts non-interactively by piping stdin:
      - Select a detected agent (simulate PATH with a fake executable dir).
      - Accept default detach shortcut (press Enter).
    - Assert created `agency.toml` contains selected default and optional keybinding.
    - Assert output mentions `agency defaults` and `agency init`.
  - Done: Added `setup_creates_global_config_via_wizard` covering non-interactive flow.
- [x] Tests: Re-running setup updates existing config and warns
  - Pre-create a global config with a known default agent and detach shortcut.
  - Run `agency setup` selecting a different agent and/or shortcut.
  - Assert `agency.toml` reflects the new selections and preserves unrelated keys.
  - Assert output includes a warning about modifying an existing config.
  - Done: `setup_updates_existing_config_and_warns` asserts warning + preservation.
- [x] Tests: `agency defaults` prints embedded TOML
  - Assert header line and that body contains keys from `defaults/agency.toml` (e.g., `[agents.claude]`).
  - Done: `defaults_prints_embedded_config` validates header and sample key.
- [x] Tests: `agency init` scaffolds files after confirmation
  - Override CWD to a temp workdir (`common::tmp_root()`).
  - Pipe confirmation “y”.
  - Assert `.agency/`, `.agency/agency.toml`, `.agency/setup.sh` exist; check `setup.sh` is executable.
  - Assert output overview lists created files.
  - Done: `init_scaffolds_files_after_confirmation` verifies scaffolding.
- [x] Add `utils/which.rs`
  - Implement `fn which(prog: &str) -> Option<PathBuf>` searching `PATH`.
  - Implement `fn is_executable(p: &Path) -> bool`.
  - Done: Added helper with PATH-aware lookup in `utils/which.rs`.
- [x] Add `utils/wizard.rs`
  - ASCII logo renderer and themed prompt helpers (list/text confirmations).
  - Central theming using `owo-colors` consistent with project log tokens.
  - Done: Implemented prompt wrapper + non-TTY fallbacks in `utils/wizard.rs`.
- [x] Add `texts/setup.rs`
  - Setup-specific strings: welcome text, agent detection/selection explainer, detach shortcut guidance, final tips.
  - Done: Strings captured in `crates/agency/src/texts/setup.rs`.
- [x] Implement `commands/defaults.rs`
  - Print a small colored header + `include_str!(defaults/agency.toml)` to stdout.
  - Done: Command prints header and embedded TOML via `defaults::run`.
- [x] Implement `commands/init_project.rs`
  - Confirm (“Generate .agency folder here? [y/N]”).
  - Create directories/files if confirmed.
  - Print a compact overview of created paths.
  - Done: Creates `.agency/` scaffolding with executable `setup.sh`.
- [x] Implement `commands/setup.rs`
  - Display logo + welcome.
  - Detect agents: iterate config agents’ `cmd[0]`, test with `which`.
  - If none detected, still allow selection from all; warn user.
  - Present list prompt with `inquire::Select`.
  - Prompt optional detach shortcut with `inquire::Text` (pre-filled).
  - Build a minimal TOML overlay with chosen `agent` and `[keybindings] detach = "..."` only when changed.
  - Write global XDG config path (ensure parent exists); if file exists, update/merge and log a warn.
  - Print final summary + tips (`agency defaults`, `agency init`, `agency --help`, quick starts).
  - Done: Implemented in `commands/setup.rs` with wizard + merge logic.
- [x] Wire CLI and first-run path
  - In `lib.rs`, add subcommands `Setup`, `Defaults`, `InitProject`.
  - Before TUI autostart on `None`, check “global config missing?”; run `setup` on TTY.
  - If not a TTY, print instruction to run `agency setup`.
  - Done: `lib.rs` handles new subcommands and first-run guard.
- [x] Add `config.rs` helpers
  - `fn xdg_config_path() -> PathBuf` (using `xdg::BaseDirectories`).
  - `fn global_config_exists() -> bool`.
  - Done: Added `global_config_path` and `global_config_exists` helpers.
- [x] `just check` and `just fmt`
  - Resolve warnings; ensure no pedantic lints outstanding.
  - Done: Executed `just check`, `just test`, and `cargo fmt`.

## Questions

1. Should first-run setup auto-run only when invoking `agency` with no subcommand, or also for any subcommand? Default: only when no subcommand, to avoid surprising behavior for scripted use.
2. What’s the preferred default agent if none is installed? Default: allow choosing from all configured agents, but highlight detected ones first; if user chooses a non-detected agent, warn and continue.
3. ASCII logo size/style constraints? Default: small (3–6 lines), readable in 80 columns, uses `owo-colors` for accents.
4. Where should `agency init` place the initial `.agency/agency.toml` content? Default: create an empty file to encourage local overrides; guide users to `agency defaults` for reference.
5. Should `setup` overwrite an existing global config? Default: if config exists, update in place by merging the new selections with existing values and warn the user that their config was modified; no force flag needed.
6. Do we prefer `inquire` for prompts over full `ratatui` wizard? Default: yes, keep it simple and consistent with CLI; we retain ratatui for the main TUI.
