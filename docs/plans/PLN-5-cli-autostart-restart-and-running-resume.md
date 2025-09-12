# PLAN-5: CLI autostart, `daemon restart`, start-by-default, and running-resume

Date: 2025-09-12

## Decisions (confirmed)

- Autostart daemon: Any CLI command that requires the daemon will automatically start it if not running.
- Restart command: Add `agency daemon restart` to streamline development workflows.
- Start-by-default on `new`: `agency new` starts the task immediately by default; pass `--draft` to keep it in Draft.
- Resume running tasks: When the daemon starts, it scans tasks and resumes all with status `running` (spawns PTY as needed).

## Scope

Deliver a cohesive UX where users don’t need to manually start the daemon, development iteration is fast with a single restart command, task creation starts work immediately by default, and daemon restarts recover previously running tasks.

- CLI auto-starts daemon on demand.
- New `daemon restart` subcommand.
- `new` starts tasks unless `--draft` is specified.
- Daemon boot performs a resume pass for all `running` tasks.

## Changes by module

### crates/cli (args, lib, rpc, tests)

- `src/args.rs`
  - Add `Daemon::Restart` subcommand with help text "Restart the daemon".
  - Update `NewArgs` to include `--draft` flag (default off) with help text "Create task without starting it".
  - Update top-level help to reflect autostart behavior and new restart command.

- `src/lib.rs`
  - Introduce helper `ensure_daemon_running()` used by all commands that depend on the daemon (e.g., `new`, `start`, `status`, `attach`, etc.).
    - Behavior: check `daemon.status`; if not reachable, invoke background `daemon start` and wait until responsive with a short bounded retry window.
    - Be silent by default for deterministic tests; optionally emit a brief note under a verbose flag later if needed.
    - Handle races where multiple CLI processes start concurrently (treat EADDRINUSE as success and re-check status).
  - Implement handler for `daemon restart`: best-effort `daemon stop` followed by background `daemon start` and readiness wait.
  - Change `new` flow: if `--draft` is absent, call `task.start` immediately after successful `task.new` and print resulting status accordingly.

- `src/rpc/client.rs`
  - No protocol changes required.
  - Optionally add a small utility `wait_until_daemon_ready(timeout)` that polls `daemon.status` via the existing client.

- `tests/`
  - Update snapshots and E2E tests:
    - Remove explicit `agency daemon start` calls where not needed; rely on autostart.
    - Add tests for `daemon restart` happy path.
    - Adjust `friendly_errors.rs`: instead of instructing users to run `agency daemon start`, assert that the CLI attempts autostart and either succeeds or reports "daemon not reachable" only after a bounded retry.

### crates/core (daemon, adapters, tests)

- `src/daemon/mod.rs`
  - On startup, after binding the socket and initializing services, scan the tasks directory for tasks with YAML status `running` and spawn PTY sessions for each, reusing the same internal routine as `task.start`.
  - Ensure idempotency and safety: log and skip tasks that fail to spawn due to missing prerequisites; do not mutate their status on boot failures.
  - Keep RPC surface unchanged; `daemon.restart` is not required as CLI composes stop+start.

- `src/domain/task.rs` and `src/adapters/fs.rs`
  - Validate utilities exist to list tasks and resolve their worktree paths for PTY spawn during resume.
  - If listing helpers are missing, add minimal internal functions to iterate tasks safely.

- `tests/`
  - Core integration test: create a task, mark it `running` via `task.start`, stop daemon, start daemon again, then assert that the PTY session is available and `pty.attach` succeeds.

### docs

- `docs/plans/` and CLI help strings reflect new behavior and `daemon restart` command.
- Consider a short note in `ADR-1-mvp.md` about autostart and resume semantics for clarity.

## Testing strategy

- CLI
  - Autostart: invoke `agency status` or `agency new` in a fresh temp environment without starting the daemon; assert the command succeeds and `daemon: running (...)` appears when queried.
  - Restart: `agency daemon restart` transitions from running → stopped → running; assert final status shows running and socket/pid change as expected.
  - New default start: `agency new` without `--draft` results in a task with `running` status; with `--draft`, status remains `draft`.
  - Friendly errors: simulate conditions where daemon cannot start (e.g., invalid socket dir in test config); assert bounded retries and clear final message.

- Core
  - Resume on boot: after marking a task `running` and stopping the daemon, restart and `pty.attach` must succeed without calling `task.start` again.

- General
  - Maintain deterministic outputs in tests (avoid noisy progress messages during autostart; rely on status lines for assertions).

## Acceptance criteria

- Running CLI commands that depend on the daemon auto-start it when not running.
- `agency daemon restart` is available and works cross-platform in tests.
- `agency new` starts tasks by default; `--draft` preserves Draft state.
- After daemon restarts, previously `running` tasks are resumed automatically and are attachable.
- `just check` and `just test` pass with updated snapshots.

## Risks and mitigations

- Race conditions on autostart with concurrent CLI invocations.
  - Mitigation: check-then-start with best-effort start, treat EADDRINUSE as success, and re-check readiness.
- Resume failures due to inconsistent repo/worktree state.
  - Mitigation: log and skip on boot; do not downgrade status; surface warnings in logs, not in user output.
- Test flakiness around readiness waits.
  - Mitigation: small, bounded retry loops with generous but quick timeouts; avoid relying on sleep-only, prefer status polling.

## Out of scope (follow-ups)

- Verbose user-facing progress messages for autostart (can be added later).
- A dedicated `daemon.restart` RPC (CLI composition is sufficient now).
- Complex failure recovery (e.g., auto-downgrading status on repeated boot failures).
