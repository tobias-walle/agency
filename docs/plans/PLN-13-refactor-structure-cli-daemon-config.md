# PLN-13: Refactor structure for CLI, Daemon, and Config

## Context

This plan restructures modules to lower cognitive load, improve cohesion, and make testing simpler, while preserving public APIs. It focuses on splitting oversized files, aligning modules with responsibilities, and keeping crate boundaries intact.

## Current Structure (high level)

- apps/
  - agency/
    - src/main.rs (boot + logging + CLI run)
    - tests/smoke.rs
- crates/
  - cli/
    - src/
      - lib.rs (command dispatch, daemon mgmt, attach loop, helpers, inline tests)
      - args.rs (clap types)
      - rpc/client.rs (HTTP-over-UDS JSON-RPC client + session)
      - stdin_handler.rs (bindings + non-consuming reader)
      - term_reset.rs (terminal reset footer)
      - rpc/mod.rs (module)
    - tests/ (integration tests for CLI behaviors)
  - core/
    - src/
      - lib.rs (module surface)
      - adapters/
        - fs.rs (layout helpers)
        - git.rs (worktrees + branch helpers)
        - pty/
          - registry.rs, session.rs, spawn.rs, sanitize.rs
        - pty.rs (public façade; attach/read/input/resize/detach)
      - agent/
        - runner.rs (env building, token substitution, agent action resolution)
      - config/
        - mod.rs (types, defaults, load/merge, validate, paths, write-default)
      - daemon/
        - mod.rs (server setup, accept loop, RPC registration, resume)
      - domain/
        - task.rs (Task model, transitions, file format)
      - logging/
        - mod.rs (tracing JSON logs)
      - rpc/
        - mod.rs (DTOs)
    - tests/ (core daemon, tasks, git worktrees, pty)
  - mcp/
    - src/lib.rs
  - test-support/
    - src/lib.rs (helpers)
- docs/
  - plans/* (implementation plans)
  - adr/*, prd/*, guides/*

## Test Inventory (nextest discovery)

- Nextest run (default profile): 74 tests across 23 binaries
- apps/agency:
  - Files: tests/smoke.rs
- crates/cli/tests (integration) files:
  - attach_e2e.rs
  - attach_fast_input.rs
  - attach_no_replay.rs
  - attach_resets_terminal.rs
  - autostart.rs
  - daemon_restart.rs
  - daemon_running.rs
  - friendly_errors.rs
  - init_scaffold.rs
  - new_auto_attach.rs
  - new.rs
  - snapshot_cli.rs
- crates/core/tests (integration) files:
  - daemon_e2e.rs
  - daemon_resume.rs
  - git_worktrees.rs
  - pty.rs
  - tasks.rs

Note: Unit tests live inside source modules (e.g., config, domain/task, agent/runner, adapters, logging). Nextest counts all runnable tests including doc-tests and integration binaries.

## Goals

- Reduce oversized files by splitting responsibilities into cohesive modules (`cli/src/lib.rs`, `core/src/daemon/mod.rs`, `core/src/config/mod.rs`).
- Keep public APIs stable to avoid broad downstream changes.
- Maintain—and ideally improve—test coverage while refactoring.

## Phases and Tasks

### [x] Phase 1: CLI extraction

- Create `crates/cli/src/commands/` with one file per user-facing command:
  - daemon.rs (status/start/stop/run/restart)
  - init.rs
  - new.rs
  - start.rs
  - status.rs
  - attach.rs
  - path.rs
  - shell_hook.rs
- Create `crates/cli/src/util/` with helpers:
  - editor.rs (`edit_text`)
  - task_ref.rs (`parse_task_ref`)
  - detach_keys.rs (`parse_detach_keys`)
  - base_branch.rs (`resolve_base_branch_default`)
  - daemon_proc.rs (`resolve_socket`, `ensure_daemon_running`, `spawn_daemon_background`)
  - errors.rs (`render_rpc_failure`)
- Update `lib.rs` to only:
  - parse args, dispatch to `commands::*`, and re-export modules as needed.
- Keep `stdin_handler.rs`, `term_reset.rs`, and `rpc/client.rs` unchanged.
- Ensure unit tests in `lib.rs` move to relevant command/util modules or a new `tests` mod.

Acceptance: `just test` passes; CLI binary behavior unchanged (manual spot-check: `daemon status`, `init`, `new --draft`, `attach`).

### [x] Phase 2: Daemon modularization

- Move accept/bind/stop-channel setup to `core/src/daemon/server.rs`.
- Create `core/src/daemon/api/` and split:
  - daemon.rs: `daemon.status`, `daemon.shutdown`
  - tasks.rs: `task.new`, `task.status`, `task.start`
  - pty.rs: `pty.attach`, `pty.read`, `pty.tick`, `pty.input`, `pty.resize`, `pty.detach`
- Move task helper functions:
  - `next_task_id`, `read_task_info`, `find_task_path_by_ref` -> `task_index.rs`
- Move resume-on-start logic -> `resume.rs`
- Keep `mod.rs` as a façade orchestrating server + api registration; preserve `start()` API.

Acceptance: Core integration tests still pass (`daemon_e2e`, `daemon_resume`, `pty`, `git_worktrees`, `tasks`).

### [ ] Phase 3: Config split

- Create:
  - `types.rs` (Config, PtyConfig, AgentConfig, LogLevel)
  - `defaults.rs` (builtin agents)
  - `load.rs` (global/project read, merge, `load()` and test helper `load_from_paths`)
  - `validate.rs` (agent validation and default agent checks)
  - `paths.rs` (global/project config paths, socket resolution helpers)
  - `write.rs` (`write_default_project_config`)
- `mod.rs` re-exports the public types and functions.
- Keep public function signatures unchanged.

Acceptance: All `config` unit tests pass; dependent modules compile unchanged.

### [ ] Phase 4: Optional polish

- Introduce `AttachmentId(String)` newtype in `adapters::pty` and mirror in `rpc` DTOs (non-breaking if via `From/Into<String>` where needed). Consider deferring if it cascades too much churn.
- Extract small `pty::constants` for history/replay limits, referenced by `session` and façade.
- Split `agent/runner.rs` into `resolve.rs` and `env.rs` only if future growth demands.

Acceptance: No behavior change; types self-document intent.

## Risk Management

- Refactor in small, compiling steps with green tests after each phase.
- Preserve function signatures and module re-exports to minimize import churn.
- Use `cargo nextest` and targeted runs for fast feedback, especially PTY/daemon tests.

## Validation Checklist

- All unit and integration tests remain green (nextest: 74 tests across 23 binaries).
- Manual smoke: `agency daemon run` + `agency new --draft test-x` + `agency start <id>` + `agency attach <id>` detaches cleanly.
- `cargo clippy` and `cargo fmt` clean post-move.
