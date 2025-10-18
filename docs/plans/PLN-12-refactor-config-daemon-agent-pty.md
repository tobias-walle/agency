# PLN-12: Config, Daemon, Agent Env, and PTY Refactor (Phased)

## Goal

Resolve current test failures and Clippy warnings, then incrementally improve robustness and readability across config generation, daemon RPC handlers, agent environment construction, and minor hygiene in PTY/CLI, ensuring tests and lint stay green after each phase.

## Outcome

- All tests pass and `cargo clippy --tests` emits no warnings.
- `agency init` produces a valid TOML without duplicate sections.
- Daemon RPC handlers avoid `unwrap()` on serialization.
- Agent environment builder has fewer parameters and clearer call sites.
- Reduced unnecessary `unsafe` and `#[allow(dead_code)]` in production code.

## Phases

### Phase 1: Stabilize Tests & Lints (<= 0.5 day) ✅

- Fix `write_default_project_config()` to avoid duplicate `[pty]` sections.
  - Implemented by appending comments that reference `pty.detach_keys` instead of adding a new `[pty]` header.
- Remove unnecessary `unsafe { std::env::set_var/remove_var }` in `config` tests.
  - Implemented by adding a test-only helper `resolve_socket_path_for(...)` and updating tests to avoid global env mutation.
- Run `cargo fmt --all` and make small formatting fixes where needed.
  - Completed.
- Validate: `just test`, `cargo clippy --tests` all green.

=> ✅ Completed: full suite passes; clippy clean for tests.

### Phase 2: Daemon RPC Serialization Robustness (<= 0.25 day) ✅

- Replaced `serde_json::to_value(...).unwrap()` in daemon handlers with `serde_json::json!(...)` for DTOs (`DaemonStatus`, `TaskInfo`, `TaskListResponse`, `TaskStartResult`, `PtyAttachResult`, `PtyReadResult`).
- Left `json!(true)` for boolean returns.
- Validation: Ran targeted tests (`daemon_e2e`, `tasks`, `pty`) and full suite; all passing.
- Clippy: `cargo clippy --tests` clean.

### Phase 3: Agent Env Builder Refactor (<= 0.5 day) ✅

- Replaced `build_env(...)` (8 args) with `BuildEnvInput` struct for clarity.
- Updated call sites in `daemon::start` and `agent::runner` tests.
- All unit tests in `agent::runner` remain readable and pass.
- Validation: `just test`, `cargo clippy --tests` all green.

### Phase 4: Hygiene in PTY/CLI (<= 0.25 day) ✅

- Removed unnecessary `#[allow(dead_code)]` from `PtySession.child` and prefixed unused constants in `term_reset`.
- Kept behavior unchanged; reduced noise per guide.
- Validation: `pty` and CLI attach tests passing.

### Phase 5: Docs & Crate-Level Summaries (<= 0.25 day) ✅

- Added concise crate-level `//!` docs for `crates/core` and `crates/cli` summarizing purpose and quick start.
- Noted `agency init` config changes (comment placement) in `agency-core` crate docs.
- Validation: `cargo doc -p agency-core -p cli` builds; tests remain green.

## Acceptance Criteria

- Phase 1: All attach-related CLI tests pass; no duplicate TOML sections; clippy emits no warnings.
- Phase 2: No `unwrap()` panics from RPC serialization paths; tests unchanged but more robust.
- Phase 3: `build_env` refactor eliminates `too_many_arguments` warning; tests updated and passing.
- Phase 4: Reduced unnecessary `allow`/`unsafe`; behavior unchanged; tests passing.
- Phase 5: Crate-level docs added without impacting builds.

## Rollout Notes

- After each phase, run `just test` and `cargo clippy --tests` to ensure stability.
- Keep commits conventional and scoped per phase for clean history.
- Avoid broad changes; focus on surgical updates aligned with the guide.
