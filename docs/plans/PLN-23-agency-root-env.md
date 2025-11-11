# PLAN: Add $AGENCY_ROOT env and use in Codex
Expose the main repo root as $AGENCY_ROOT for agent commands; pass it to Codex via --add-dir.

## Goals
- Inject $AGENCY_ROOT into the session env with the main git repo root (not the linked worktree)
- Expand $AGENCY_ROOT in agent command templates
- Default Codex command includes --add-dir $AGENCY_ROOT
- Add a small unit test to ensure $AGENCY_ROOT is populated

## Out of Scope
- Changing behavior of non-Codex agents
- Adding $AGENCY_ROOT to bootstrap scripts or other commands
- Broader permission elevation beyond Codex’s --add-dir
- Extensive refactors of start/attach logic

## Current Behavior
- Start builds env from process vars and adds $AGENCY_TASK only:
  - crates/agency/src/commands/start.rs:48-51
- Main repo root (not worktree) is already computed for the project key:
  - crates/agency/src/commands/start.rs:28-33 (via open_main_repo + repo_workdir_or)
  - Helper implementations:
    - crates/agency/src/utils/git.rs:1 (open_main_repo)
    - crates/agency/src/utils/git.rs:15 (repo_workdir_or)
- Agent argv is expanded using <root> and $VARS from a context env map:
  - crates/agency/src/utils/cmd.rs:1,31
  - Context built with repo_root and env_map: crates/agency/src/commands/start.rs:63-70
- Environment passed to PTY child is taken from WireCommand.env (so we control it):
  - crates/agency/src/pty/session.rs:116-133 (build_pty_command_for)
- Default Codex command does not use --add-dir:
  - crates/agency/defaults/agency.toml:25-26

## Solution
- Compute AGENCY_ROOT as the canonicalized main repo workdir (same as ProjectKey root)
- Insert AGENCY_ROOT into env_map before command expansion and session open
- Update defaults for Codex to include --add-dir $AGENCY_ROOT
- Add a tiny helper in start.rs to build the base env map (injecting AGENCY_TASK and AGENCY_ROOT) and unit test it

## Architecture
- Modify
  - crates/agency/src/commands/start.rs
    - Add helper `build_session_env(repo_root: &Path, task_description: &str) -> HashMap<String,String>`
    - Use it to set `env_map` and feed into `CmdCtx` and `WireCommand`
  - crates/agency/defaults/agency.toml
    - Change `[agents.codex].cmd` to include `--add-dir`, `$AGENCY_ROOT`
- Tests
  - crates/agency/src/commands/start.rs
    - Add unit test ensuring `build_session_env` sets `AGENCY_ROOT` and `AGENCY_TASK`

## Detailed Plan
- [ ] Add `build_session_env` in start.rs:
  - Inputs: `repo_root: &Path`, `task_description: &str`
  - Behavior: clone `std::env::vars()`, insert `("AGENCY_TASK", trimmed_description)`, insert `("AGENCY_ROOT", canonical_repo_root_string)`
  - Return HashMap
- [ ] Replace current env_map construction in start.rs with `build_session_env`
  - Use same canonicalized repo_root for CmdCtx and `AGENCY_ROOT`
- [ ] Update defaults/agency.toml Codex agent:
  - From: `["codex", "--full-auto", "$AGENCY_TASK"]`
  - To: `["codex", "--full-auto", "--add-dir", "$AGENCY_ROOT", "$AGENCY_TASK"]`
- [ ] Add unit test in start.rs:
  - Create a temp directory as fake repo root
  - Call `build_session_env(repo_root, " Body ")`
  - Assert `env["AGENCY_ROOT"] == canonical(repo_root)` and `env["AGENCY_TASK"] == "Body"`
- [ ] Run `just check` and fix any lints, then `cargo fmt`
- [ ] Optionally sanity-run `just test` to ensure all tests pass

## Questions
1) Should the env var be exactly named AGENCY_ROOT? Default: Yes.
2) Should AGENCY_ROOT be canonicalized (resolving symlinks)? Default: Yes, for consistency with ProjectKey use.
3) Should we also add --add-dir $AGENCY_ROOT to other agents by default? Default: No, only Codex for now.
4) Should bootstrap commands also receive AGENCY_ROOT in env? Default: No change; out of scope.
5) If run inside a linked worktree, should AGENCY_ROOT still point to the main repo root? Default: Yes (explicit requirement).
6) If outside a git repo, what should happen? Default: Error as today (start already fails when no repo).
7) Preferred Codex arg order? Default: `codex --full-auto --add-dir $AGENCY_ROOT "$AGENCY_TASK"` to preserve existing flags first.

