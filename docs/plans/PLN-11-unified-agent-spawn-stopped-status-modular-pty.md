# PLN-11: Unified Agent Spawn, Stopped Status, No Injection, Modular PTY

Date: 2025-09-14

Unify agent process spawning behind a single path, add a Stopped status to change restart semantics, remove CLI injection, and modularize PTY code.

## Goals

- Unify spawning of `opencode` and `sh` via a single generic runner.
- Start `opencode` directly in the PTY without an intermediate shell or injection.
- Auto-attach after `new` only when stdout is a TTY (unless `--no-attach`).
- Introduce `Stopped` status and mark `Running -> Stopped` on daemon restart.
- Keep `sh` available through the same agent mechanism for debugging.
- Embed agent configuration in `config.toml` using command arrays with `$AGENCY_*` placeholders.
- Refactor PTY adapter into a more modular structure for readability and evolution.

## Non Goals

- Implement a TOML agent registry loader beyond what is needed now.
- Persist or synchronize environment variables to disk.
- Add new agents beyond `opencode`, `shell`, and existing `fake` mapping.
- Change RPC transport or add new RPCs beyond what is needed for start/attach.

## Current Behavior

- PTY spawns `sh` by default in `crates/core/src/adapters/pty.rs`:
  ```rust
  // crates/core/src/adapters/pty.rs (ensure_spawn)
  let mut cmd = CommandBuilder::new("sh");
  cmd.cwd(worktree_path.as_os_str());
  let child = pair.slave.spawn_command(cmd)?;
  ```
- Daemon `task.start` creates the worktree, transitions to `Running`, and calls `ensure_spawn`:
  ```rust
  // crates/core/src/daemon/mod.rs (task.start)
  let wt = gitutil::ensure_task_worktree(&repo, &root, id, &slug, &task.front_matter.base_branch)?;
  task.transition_to(Status::Running)?;
  fs::write(&path, task.to_markdown()?)?;
  { let _ = crate::adapters::pty::ensure_spawn(&root, id, &wt); }
  ```
- CLI `new` currently does a temporary attach and injects a shell command for opencode, then detaches:
  ```rust
  // crates/cli/src/lib.rs (build_opencode_injection + attach_and_maybe_inject)
  s.push_str("opencode --agent plan -p \"$(cat <<'EOF'\n");
  // ... prompt content ...
  s.push_str("\nEOF\n)\"\n");
  // attach, send initial input, short read, then detach
  ```
- Config supports `default_agent` and PTY detach keys. Agent selection exists but the opencode integration relies on injection rather than direct spawn.

## Solution

- Add `Status::Stopped` and transitions so `Running -> Stopped` (daemon restart) and `Stopped -> Running` via `task.start`.
- Persist the restart policy: during resume scan, load tasks, transition `Running -> Stopped`, write files back to disk, and do not spawn PTYs.
- Introduce a generic agent runner that resolves `(program, argv)` from config and also prepares environment variables. Provide `$AGENCY_*` token substitution for args without invoking a shell; additionally export the same values in the child process environment so agents can consume either.
- Embed agent config in `config.toml` under `[agents.<name>]` with per‑action arrays: `start`, `resume`, `run`. Ship built‑in defaults for `opencode` and `fake` (maps to `sh`). If `claude-code` is selected but not configured, fail early with a clear error.
- Use a single PTY spawn path that takes `(program, args)` and sets `cwd` to the task worktree. No intermediate shell unless the agent command is explicitly `sh`.
- In `task.start`, use the agent runner to resolve the `start` action, perform token substitution, and spawn directly. Do not log prompt contents; log sizes/flags only.
- Keep `fake` as the built‑in debugging agent mapping to `sh`. Avoid introducing a separate `shell` agent name; optionally allow `shell` as an alias to `fake` in the future.
- In CLI `new`, remove injection and temporary attach. Auto‑attach only when stdout is a TTY (and not `--no-attach`). Update tests that relied on injection.
- Refactor PTY code into modules (`spawn.rs`, `session.rs`, `registry.rs`, `sanitize.rs`) while re‑exporting the same public API from `pty::mod`. Retain `clear_registry_for_tests()` for test isolation.
- Clarify semantics: `Stopped` indicates daemon‑initiated halt after restart and requires `task.start` to run again; `Idle` remains a user‑initiated pause.

## Detailed Plan

HINT: Update checkboxes during the implementation

1. [x] Domain: add `Stopped` status and transitions
   - Update `crates/core/src/domain/task.rs` to add `Status::Stopped` (serde) and transition rules (`Running -> Stopped`, `Stopped -> Running`).
   - Expose in `crates/core/src/domain/mod.rs` if re-exports are used.
   - Update CLI rendering in `crates/cli/src/lib.rs` to include `stopped` in status mapping.
   - Add unit tests in `crates/core/tests/tasks.rs` for the new transitions and explicit `Idle` vs `Stopped` behavior.

2. [x] Daemon: persist Running -> Stopped on restart (no spawn)
   - In `crates/core/src/daemon/mod.rs::resume_running_tasks_if_configured`, scan tasks; for each `Running`, transition to `Stopped` and write the updated file back to disk.
   - Do not spawn PTYs during resume; log `daemon_resume_mark_stopped` with counts.
   - Update tests in `crates/core/tests/daemon_resume.rs` to assert: statuses changed to `Stopped`, `pty.attach` rejected until `task.start` is called again.

3. [ ] Config: embed agent command arrays in `config.toml`
   - Extend `crates/core/src/config/mod.rs` to parse `[agents.<name>]` sections with fields:
     - `display_name: Option<String>`
     - `start: Vec<String>`
     - `resume: Option<Vec<String>>` (placeholder; not used yet unless session handling is added)
     - `run: Option<Vec<String>>`
   - Provide built-in defaults for `opencode` and `fake` (maps to `sh`) when not present. If `claude-code` is selected without config, error clearly.
   - Add unit tests for parsing and precedence (project overrides global) and missing/invalid agent definitions.

4. [ ] Agent runner: resolve commands, token substitution, and env
   - New module `crates/core/src/agent/runner.rs`:
     - `build_env(...) -> HashMap<String, String>` producing `AGENCY_TASK_ID`, `AGENCY_SLUG`, `AGENCY_BODY`, `AGENCY_PROMPT`, `AGENCY_PROJECT_ROOT`, `AGENCY_WORKTREE`, optional `AGENCY_SESSION_ID`, `AGENCY_MESSAGE`.
     - `substitute_tokens(args: &[String], env: &HashMap<_,_>) -> Vec<String>` replacing `$AGENCY_*` in argv without shell; also set the same keys in the child environment.
     - `resolve_action(agent, action) -> (program: String, args: Vec<String>)` using config/defaults and validating availability.
   - Tests for substitution, env building, and action resolution (no external binaries required).

5. [ ] PTY modularization + generic spawn API
   - Split `crates/core/src/adapters/pty.rs` into `crates/core/src/adapters/pty/`:
     - `spawn.rs`: builds `CommandBuilder` from `(program, args)`, sets `cwd`, spawns into PTY.
     - `session.rs`: `PtySession` and reader loop.
     - `registry.rs`: session/attachment registry and test reset helper.
     - `sanitize.rs`: replay sanitization and alt-screen detection (moved from current file).
     - `mod.rs`: public API re-exporting `spawn_command`, `attach`, `read`, `input`, `resize`, `detach` to preserve external callers.
   - Provide `spawn_command(root, task_id, worktree, program, args)`; remove hard-coded `sh` from the primary path.
   - Keep a thin `ensure_spawn_sh(...)` helper for tests/debugging if needed and keep `clear_registry_for_tests()` accessible in tests.
   - Update imports/usages in daemon and tests.

6. [ ] Daemon: start tasks via agent runner
   - In `task.start`, construct env map and resolve the `start` action for the selected agent.
   - Perform `$AGENCY_*` substitution and call `pty::spawn_command(...)` to start the process directly.
   - Log sizes/flags, not prompt contents.
   - Tests: ensure `fake` spawns `sh`; CI must not depend on external `opencode` binary.

7. [ ] CLI: remove injection and use TTY-only auto-attach
   - In `crates/cli/src/lib.rs::new_task`, remove `build_opencode_injection` and `attach_and_maybe_inject`.
   - If `--no-attach`: return; else if stdout is TTY: call `attach_interactive(...)`; otherwise, print status only.
   - Update CLI tests: drop injection-based assertions; continue using `--agent fake --no-attach` where appropriate.

8. [ ] Docs and examples
   - Update `README.md` to document:
     - `Stopped` status semantics vs `Idle` and the restart policy (`Running -> Stopped` persisted; requires `task.start` to run again).
     - Agent config under `[agents.<name>]` in `config.toml` with per-action arrays and default availability for `opencode` and `fake`; `claude-code` requires explicit config.
     - `$AGENCY_*` token substitution semantics in argv and that the same keys are exported as environment variables in the child process.
     - Auto-attach behavior (TTY-only) and direct agent process spawn without injection.
   - Example config snippet:
     ```toml
     [agents.opencode]
     display_name = "OpenCode"
     start  = ["opencode", "--agent", "plan", "-p", "$AGENCY_PROMPT"]
     # resume is reserved for future use when session capture is implemented
     # resume = ["opencode", "--session", "$AGENCY_SESSION_ID"]
     run    = ["opencode", "run", "$AGENCY_MESSAGE"]

     [agents.fake]
     display_name = "Shell"
     start = ["sh"]
     ```

## Notes

HINT: Update this section during the implementation with relevant changes to the plan, problems that arised or other noteworthy things.

- The restart policy change (Running -> Stopped) avoids unintended mass restarts after daemon restarts.
- The agent runner abstraction decouples agent semantics from PTY and daemon, enabling future agent expansion.
- Using `$AGENCY_*` tokens avoids shell quoting; only opt into shell when explicitly required by the agent.
- Keep tests independent of external agent binaries by using the `shell`/`fake` agent in CI.
