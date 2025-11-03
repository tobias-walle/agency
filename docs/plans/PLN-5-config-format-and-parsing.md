# PLN-5: Config format and parsing

Date: 2025-11-03

Introduce a TOML configuration with layered precedence and parsing at startup.

## Goals

- Define a minimal TOML schema with `[agents]` as a Map where each key is an agent name (e.g. `opencode`) and each value is an `AgentConfig`.
- Support `AgentConfig { cmd: Vec<String> }` where arrays and maps default to empty and other fields are optional.
- Implement layered loading with precedence: repository defaults < XDG global `~/.config/agency/agency.toml` < project `./.agency/agency.toml`.
- Parse the merged TOML into `AgencyConfig` at startup and make it available, without changing command behavior yet.
- Provide a default `agency.toml` in the repo (with comments) and link it from `README.md`.
- Standardize filenames to `agency.toml` for both global and project config and update docs accordingly.

## Non Goals

- Executing agent commands or substituting `$AGENCY_TASK`.
- Adding more options beyond `agents.*.cmd`.
- Advanced schema validation beyond successful TOML parsing.
- Runtime reloading and Windows support.

## Current Behavior

- `crates/agency/src/config.rs` defines `AgencyConfig { cwd }` that manages paths for `.agency/tasks` and `.agency/worktrees`.
- `crates/agency/src/lib.rs` creates this paths config in `run()` and there is no external configuration.
- `README.md` mentions `config.toml` paths, which should be standardized to `agency.toml`.

## Solution

- Add a repository default TOML file (`crates/agency/defaults/agency.toml`) with comments and embed via `include_str!`.
- Introduce parsed config types:
  - `AgencyConfig { agents: BTreeMap<String, AgentConfig> }` (maps default to empty).
  - `AgentConfig { cmd: Vec<String> }` (arrays default to empty).
- Make all options optional except arrays and maps which default to empty values via `#[serde(default)]`.
- Implement `load_config(cwd: &Path)`:
  - Start from embedded defaults.
  - If present, deep-merge XDG global `agency.toml` (respect XDG on macOS/Linux).
  - If present, deep-merge project `./.agency/agency.toml`.
  - Use deep merge for tables and last-wins replacement for arrays and scalars.
  - On invalid TOML in a present file, fail with `bail!` and a clear message.
- Rename the existing paths struct to `AgencyPaths` and keep its helpers unchanged.
- Create a simple runtime context `AppContext { paths: AgencyPaths, config: AgencyConfig }` and construct it in `run()`.
- Do not consume `config` in commands yet.
- Update `README.md` to document precedence and link the default file.

## Detailed Plan

- HINT: Update checkboxes during the implementation

1. [ ] Add dependencies
   - `serde` with `derive` feature, `toml`, `xdg`.
   - Commands: `cargo add serde --features derive && cargo add toml && cargo add xdg`.

2. [ ] Add default config file in repo
   - File: `crates/agency/defaults/agency.toml`.
   - Contents with comments:
     ```toml
     # Agency default configuration
     # [agents.opencode]
     # The command used to invoke the opencode agent.
     # "$AGENCY_TASK" is a placeholder for the task text.
     [agents.opencode]
     cmd = ["opencode", "--prompt", "$AGENCY_TASK"]
     ```
   - Embed with:
     ```rust
     const DEFAULT_TOML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/defaults/agency.toml"));
     ```

3. [ ] Define parsed config types
   - File: `crates/agency/src/config.rs`.
   - Add:
     ```rust
     use serde::Deserialize;
     use std::collections::BTreeMap;

     #[derive(Debug, Clone, Default, Deserialize)]
     pub struct AgentConfig {
       #[serde(default)]
       pub cmd: Vec<String>,
     }

     #[derive(Debug, Clone, Default, Deserialize)]
     pub struct AgencyConfig {
       #[serde(default)]
       pub agents: BTreeMap<String, AgentConfig>,
     }
     ```

4. [ ] Implement deep-merge helper
   - Add `merge_values(base: &mut toml::Value, overlay: toml::Value)` that merges tables recursively and replaces arrays/scalars.

5. [ ] Implement `load_config(cwd: &Path)`
   - Parse `DEFAULT_TOML` into `toml::Value`.
   - Resolve XDG path with `xdg::BaseDirectories::with_prefix("agency")` and `find_config_file("agency.toml")`.
   - If present, parse and merge.
   - If `cwd.join(".agency/agency.toml")` exists, parse and merge.
   - Deserialize merged value into `AgencyConfig` and return it.

6. [ ] Rename paths struct and wire context
   - Rename existing `AgencyConfig { cwd }` to `AgencyPaths`.
   - Create `AppContext { paths: AgencyPaths, config: AgencyConfig }`.
   - In `crates/agency/src/lib.rs::run()`, build `AppContext` by calling `load_config(&cwd)` and pass it to commands (even if unused for now).

7. [ ] Tests for precedence and defaults
   - File: `crates/agency/tests/config.rs`.
   - Strategy: use `tempfile` for temp dirs and override XDG with `.env("XDG_CONFIG_HOME", temp_dir)`.
   - Cases:
     - Defaults only: no files present; expect `agents.opencode.cmd == ["opencode", "--prompt", "$AGENCY_TASK"]`.
     - Global override: write `agency.toml` under `$XDG_CONFIG_HOME/agency/agency.toml`; expect it overrides defaults.
     - Project override: write `./.agency/agency.toml`; expect it overrides global.
     - Missing keys: partial tables; expect maps default to `{}` and arrays to `[]`.
     - Invalid TOML in a present file: expect failure with actionable error.

8. [ ] Documentation
   - Update `README.md` to standardize on `agency.toml`, document search order, note XDG behavior, and link to `crates/agency/defaults/agency.toml`.

9. [ ] QA
   - Run `just check` and `just test`.
   - Manually run `just agency` under different config setups and ensure no behavior changes beyond parsing.

## Notes

- Arrays and maps default to empty via `#[serde(default)]` to satisfy optional semantics.
- Arrays and scalars are replaced on overlay; tables are deep-merged with last-wins.
- Unknown keys are preserved through merge but ignored by the typed parser.
- We only parse and store the config; execution and validation come later.
