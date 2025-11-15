# PLAN: Persistent Interactive Shell Wrapper For Agents
Make agent sessions keep a real interactive shell so Ctrl-C and Ctrl-Z return to the shell prompt, across bash, zsh, fish and nu.

## Goals
- Keep pane top process as an interactive shell (not `sh -c payload`).
- Launch agent command inside the shell via tmux keys, not shell parsing.
- Use a POSIX `run-agent.sh` wrapper to avoid cross-shell quoting issues.
- Ensure `.git/info/exclude` always ignores `run-agent.sh`.
- Reduce duplication between `start` and `attach` by centralizing logic.

## Out of scope
- Changing tmux theme or UI status rendering.
- Non-tmux execution modes.
- Windows support.

## Current Behavior
- Agents start with the user shell executing `-c <payload>` and exit when the payload exits; Ctrl-C or Ctrl-Z ends the job and also exits the shell rather than returning to a prompt.
  - `crates/agency/src/commands/start.rs:71–80` builds `-c payload` and passes it into `tmux::start_session`.
  - `crates/agency/src/commands/attach.rs:71–80` does the same when autostarting.
- tmux session creation runs `program args` as the pane process and sets `remain-on-exit off`:
  - `crates/agency/src/utils/tmux.rs:124` new-session call.
- Interactive boundary for CLI/TUI uses `utils::interactive::scope` to re-init terminal, but does not affect in-session signal behavior.
  - `crates/agency/src/tui/mod.rs:133` sender; `Begin/End` handling at `:145–153`.

## Solution
- Start tmux sessions with a persistent interactive shell as the pane process (no `-c`).
- Write a task-local wrapper script `run-agent.sh` with `#!/bin/sh` and `exec <program> <args...>`.
- Inject session environment via tmux `set-environment -t <session>` for `AGENCY_TASK`, `AGENCY_ROOT`, `AGENCY_TASK_ID`.
- Send the absolute path to `run-agent.sh` into the shell using `tmux send-keys` followed by Enter.
- Always add ignore entries for this script to `.git/info/exclude` — both a generic pattern and the exact repo-relative path.
- Centralize shared logic (plan build and session start) into a new `utils/session.rs` and use it from `start` and `attach`.

## Architecture
- New
  - `crates/agency/src/utils/session.rs`
    - `SessionPlan { task_meta, repo_root, worktree_dir, agent_program, agent_args, env_map, shell_argv }`
    - `build_session_plan(ctx, &TaskRef) -> Result<SessionPlan>`
    - `start_session_for_task(ctx, &SessionPlan, attach: bool) -> Result<()>`
- Modified
  - `crates/agency/src/utils/tmux.rs`
    - Add `send_keys(cfg, target, text)` and `send_keys_enter(cfg, target)` helpers.
    - Add `tmux_set_env_local(cfg, target, key, value)` helper.
  - `crates/agency/src/utils/git.rs`
    - Add `git_dir_at(cwd) -> Result<PathBuf>`.
    - Add `ensure_exclude_contains(cwd, patterns: &[&str]) -> Result<()>`.
  - `crates/agency/src/commands/start.rs`
    - Replace inlined argv/env/tmux launch with shared `session` helpers.
  - `crates/agency/src/commands/attach.rs`
    - Join existing session when present; otherwise delegate to shared helpers for autostart.

## Testing
- Unit
  - `tmux::parse_detach_binding` unchanged sanity tests keep passing.
  - Add tests for `utils::command::as_shell_command` already present; no change needed.
  - Add tests for `utils::git::ensure_exclude_contains` to append missing lines and preserve existing ones.
  - Add tests for `utils::tmux::send_keys` argument building (use dummy command formatting with no tmux execution).
- Integration
  - CLI tests: start then attach flows succeed and do not error; mock tmux by checking constructed commands via helper seams.
  - Verify `.git/info/exclude` contains both generic and specific wrapper script patterns after start/attach autostart.
- E2E
  - Manual verification: Ctrl-C and Ctrl-Z return to shell prompt in tmux pane across bash/zsh/fish/nu.

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
[ ] Add `utils/tmux.rs` helpers: `send_keys`, `send_keys_enter`, `tmux_set_env_local`.
[ ] Add `utils/git.rs` helpers: `git_dir_at`, `ensure_exclude_contains`.
[ ] Create `utils/session.rs` with `SessionPlan`, `build_session_plan`, `start_session_for_task`.
[ ] Implement writing `run-agent.sh` to `<worktree>/.agency/state/run-agent.sh` with `#!/bin/sh` and `exec <program> <args...>`.
[ ] In `start_session_for_task`, call `tmux::start_session` with `shell_argv` (no `-c`) and then `send_keys` to run the script.
[ ] In `start_session_for_task`, set env via `tmux_set_env_local` and ensure `.git/info/exclude` patterns via `ensure_exclude_contains`.
[ ] Refactor `commands/start.rs` to use `build_session_plan` and `start_session_for_task` (preserve branch/worktree prep and attach flag).
[ ] Refactor `commands/attach.rs` to join existing or autostart via shared helpers (ensure worktree prep on autostart).
[ ] Add unit tests for new helpers; extend CLI tests to assert `.git/info/exclude` update.
[ ] Run `just check` and `just fix`; format with `cargo fmt`.
[ ] Validate manually across bash/zsh/fish/nu shells in tmux.

## Questions
1) Wrapper script path: is `<worktree>/.agency/state/run-agent.sh` acceptable?
   - Default: Yes.
2) Env injection: prefer tmux `set-environment` vs exporting in script?
   - Default: Use tmux `set-environment` to keep script minimal.
3) Generic ignore pattern for all tasks in `.git/info/exclude`?
   - Default: Add both `.agency/**/run-agent.sh` and the specific repo-relative path.
4) Attach autostart should prepare worktree like start?
   - Default: Yes; use `prepare_worktree_for_task` before starting.
5) Strict POSIX in the wrapper script (no bashisms)?
   - Default: Yes; use `#!/bin/sh` and `exec` only.

