# PLAN: Agent flag, YAML front matter, and attach selection
Add `-a/--agent` to `agency new` to choose an agent, persist it as YAML front matter, and ensure `attach` runs the chosen agent. Provide a configurable default agent via config.

## Goals
- Add `-a/--agent` to `agency new` to select an agent.
- Write YAML front matter with `serde_yaml` for the selected agent.
- Make `attach` read front matter and always run the right agent.
- Add `agent` default in config; use it when no front matter exists.
- Validate agent names against configured agents; provide helpful errors.
- Keep existing behavior intact if no agent is specified.

## Out of scope
- Interactive selection UI for agents.
- Migrating existing task files to add front matter.
- Changing task file format beyond YAML front matter and title line.
- Networked agent integrations beyond current `cmd` execution.

## Current Behavior
- `agency new <slug>` creates `.agency/tasks/{id}-{slug}.md` with only a title:
  - Write file: `crates/agency/src/commands/new.rs:29`
  - Create branch and worktree: `crates/agency/src/commands/new.rs:33`
- No agent is accepted by `new` (no `-a/--agent`):
  - CLI: `crates/agency/src/lib.rs:22`
- `attach` takes the entire task file as `AGENCY_TASK` and always uses agent `fake`:
  - Hard-coded agent selection: `crates/agency/src/commands/attach.rs:48`
  - Env expansion: `crates/agency/src/utils/command.rs`
- Config supports multiple agents but no default agent option:
  - Config types: `crates/agency/src/config.rs`
  - Defaults: `crates/agency/defaults/agency.toml`

## Solution
- Extend CLI: add `#[arg(short = 'a', long = "agent")] agent: Option<String>` to `Commands::New`.
- Update `new::run` to accept `agent: Option<&str>` and:
  - Validate `agent` exists in `ctx.config.agents`; on unknown, bail listing known agents.
  - If present, write YAML front matter using `serde_yaml`:
    - `---`
    - `agent: <name>`
    - `---`
    - blank line, then current title line.
  - If absent, keep current title only (no YAML).
- Add front matter parsing with `serde_yaml`:
  - Detect `---` at file start, read until next `---`, parse into a struct `TaskFrontmatter { agent: Option<String> }`.
  - Return both front matter and the markdown body (excluding front matter).
- Switch `attach` to:
  - Load task file, parse front matter.
  - Determine agent name: `frontmatter.agent` or `ctx.config.agent` (default) or bail with helpful error if none/invalid.
  - Validate agent exists in config; pick argv from `AgentConfig`.
  - Set `AGENCY_TASK` to the markdown body (without front matter).
- Config:
  - Add `agent: Option<String>` to `AgencyConfig`.
  - Set default in `crates/agency/defaults/agency.toml`: `agent = "fake"`.
- Dependencies:
  - `cargo add serde_yaml` (front matter serialization and parsing).
- Tests:
  - New task writes front matter when `-a` is provided.
  - Front matter parser unit tests (with and without YAML).
  - Agent resolution unit tests (front matter wins over config; validation on unknown).
  - Keep existing tests passing.

## Architecture
- `crates/agency/src/lib.rs`
  - Add `agent: Option<String>` to `Commands::New`.
  - Pass `agent.as_deref()` to `commands::new::run`.
- `crates/agency/src/commands/new.rs`
  - Change signature: `run(ctx, slug, no_edit, agent: Option<&str>)`.
  - Validate agent exists in `ctx.config.agents`.
  - Write YAML front matter via `serde_yaml` when provided.
- `crates/agency/src/commands/attach.rs`
  - Parse front matter and body from task file.
  - Choose agent name from front matter, else `ctx.config.agent`.
  - Validate and build argv from `ctx.config.agents[agent].cmd`.
  - Set `AGENCY_TASK` to the body only.
- `crates/agency/src/config.rs`
  - Extend `AgencyConfig` with `pub agent: Option<String>`.
- `crates/agency/defaults/agency.toml`
  - Add `agent = "fake"` at the root.
- `crates/agency/src/utils/task.rs`
  - Add `TaskFrontmatter` struct (serde derive).
  - Add `parse_front_matter_and_body(&str) -> (Option<TaskFrontmatter>, &str)` and a `read_front_matter_and_body(Path)` helper.
- `Cargo.toml`
  - Add `serde_yaml` via `cargo add serde_yaml`.
- `crates/agency/tests/cli.rs`
  - Add test `new_writes_yaml_header_when_agent_specified`.
- New unit tests (co-located):
  - `utils::task` tests for parsing and agent resolution helper (if added).

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
1. [ ] CLI: add `agent: Option<String>` to `Commands::New` in `crates/agency/src/lib.rs` and forward to `new::run`.
2. [ ] Config: add `agent: Option<String>` to `AgencyConfig` (deserialize default None).
3. [ ] Defaults: set `agent = "fake"` in `crates/agency/defaults/agency.toml`.
4. [ ] Dep: `cargo add serde_yaml` for YAML serialization and parsing.
5. [ ] Utils: implement `TaskFrontmatter` and parser in `crates/agency/src/utils/task.rs`.
   - [ ] Unit tests: parse with front matter, without, malformed boundaries.
6. [ ] New: update `crates/agency/src/commands/new.rs`:
   - [ ] Update signature to include `agent: Option<&str>`.
   - [ ] Validate agent exists in config when provided.
   - [ ] Write YAML front matter when provided via `serde_yaml::to_string`, then title.
7. [ ] Attach: update `crates/agency/src/commands/attach.rs`:
   - [ ] Read task file, parse front matter and body.
   - [ ] Select agent: front matter `agent` else `ctx.config.agent` else error; validate exists.
   - [ ] Use selected agent’s `cmd` template and expand env vars; set `AGENCY_TASK` to body only.
8. [ ] Tests:
   - [ ] Add integration test in `crates/agency/tests/cli.rs` verifying YAML header when using `-a fake`.
   - [ ] Add unit tests for agent selection logic if factored into a helper.
9. [ ] Run `just check`, fix issues; then `just test`.
10. [ ] Run `just fmt`.

## Questions
1) When no agent is specified on `new`, should we still emit front matter with the default agent?
   - Assumed: No. Omit front matter; `attach` will fall back to config `agent`.
2) If no default agent is configured and no front matter is present, should we fall back to `fake` or error?
   - Assumed: Default config sets `agent = "fake"`, so it will be present. If the user removes it, we bail with a clear error listing configured agents.
3) Should `AGENCY_TASK` exclude front matter?
   - Assumed: Yes. Pass only the markdown body to the agent.
4) Case sensitivity for agent names?
   - Assumed: Case-sensitive; match keys under `[agents]` exactly.
5) Should we add a CLI override for `attach` (e.g., `--agent` there too)?
   - Assumed: Not required now. `attach` uses task front matter, else default config.

