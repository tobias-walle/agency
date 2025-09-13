# PLN-10: Integrate OpenCode Agent, Editor Descriptions, Auto-Attach, Env Inheritance

Date: 2025-09-13

Add agent execution (OpenCode), editor-based task descriptions, auto-attach after creation, and centralized environment inheritance.

## Goals

- Enable running an external coding agent (OpenCode) for tasks.
- Allow providing a task description via `$EDITOR` or `-m,--message`.
- Auto-attach to the task PTY after creation and keep the user attached interactively (opt-out via `--no-attach`).
- Make agent selection configurable via project/global config (`default_agent`).
- Ensure all spawned processes inherit the initiating shell environment without storing env.
- Keep tests independent of OpenCode (no hard dependency in CI).

## Non Goals

- Implement additional agents beyond OpenCode (keep hooks for future).
- Add persistence or synchronization of environment variables.
- Overhaul RPC or transport beyond what’s needed for agent start.
- Build a general agent lifecycle manager (just initial injection).

## Current Behavior

- CLI commands are defined in `crates/cli/src/args.rs`.
  - `NewArgs` has optional `agent` (resolved via config or flag) and supports `--message` and interactive editor fallback via `$EDITOR` (defaults to `vi`).
- CLI flows are in `crates/cli/src/lib.rs`.
  - `new_task()` resolves agent from flag or config, collects `body` from `--message` or the editor when interactive, then calls `task.new` and `task.start`.
  - After start, it currently performs a non-interactive auto-attach to optionally inject an agent command, waits briefly to read, and immediately detaches. It does not keep the user attached yet.
  - `attach_interactive()` attaches and manages an interactive PTY loop.
- RPC DTOs in `crates/core/src/rpc/mod.rs`:
  - `TaskNewParams { body: Option<String>, agent: Agent, ... }` supports carrying a body.
  - `TaskStartParams` spawns PTY via daemon; `PtyAttach*`/`PtyInput` support IO.
- Daemon behavior in `crates/core/src/daemon/mod.rs`:
  - `task.start` ensures git worktree and spawns PTY running `sh` (see `adapters/pty.rs`).
  - No agent invocation; attaching is separate.
- PTY adapter in `crates/core/src/adapters/pty.rs`:
  - Spawns `sh` in the task worktree and supports single active attachment, read/input/resize.
- Config in `crates/core/src/config/mod.rs`:
  - `default_agent` exists with proper precedence; `pty.detach_keys` is present.
- Editor flow is implemented and used when interactive; users can still provide a task description via `--message` to bypass the editor.

## Solution

- Add `default_agent: Option<Agent>` to configuration (global and project) with standard precedence.
- Make `--agent` optional for `agency new`; resolve agent from flag or config, else error.
- Add description capture before `task.new` via `$EDITOR` -> `vi`, or `-m,--message` to bypass editor.
- Pass description as `body` in `TaskNewParams`.
- After successful `task.start`, auto-attach and keep the user attached interactively unless `--no-attach`.
  - If agent is `opencode`, inject a one-shot shell command into the PTY to run `opencode --agent plan -p "<prompt>"`, where `<prompt>` is `# Task: [slug]\n\n[description]` using a here-doc for safe multiline quoting, then continue in the interactive attach session.
- Centralize environment behavior by relying solely on process inheritance; do not store env (Note: The env needs to be inherited by the CLI, not the daemon. An exception is when the task was started with `daemon (re)start`).
- Add tracing logs around editor resolution, agent resolution, attach, and injection (avoid logging env contents or prompt text; log sizes/flags only).
- Keep tests using `fake` agent and `--no-attach` where needed to avoid OpenCode dependency.

## Detailed Plan

HINT: Update checkboxes during the implementation

1. [x] Config: add `default_agent`
   - Update `crates/core/src/config/mod.rs`:
     - Add `pub default_agent: Option<crate::domain::task::Agent>` to `Config`.
     - Add to `PartialConfig` and merge logic.
     - Update `Default` and `write_default_project_config()` to include commented example:
       - `# default_agent = "opencode" | "claude-code" | "fake"`.
   - Add/adjust unit tests for config merge and defaults.

2. [x] CLI args: make agent optional, add message/no-attach
   - Update `crates/cli/src/args.rs`:
     - Change `NewArgs.agent` to `Option<AgentArg>`.
     - Add `#[arg(long = "no-attach")] pub no_attach: bool`.
     - Add `#[arg(short = 'm', long = "message")] pub message: Option<String>`.
   - Update help snapshot tests if output changes.

3. [x] Editor helper
   - In `crates/cli/src/lib.rs`, add `fn edit_text(initial: &str) -> anyhow::Result<String>`:
     - Resolve editor from `$EDITOR` or fallback `vi`.
     - Write `initial` to a temp file, spawn editor inheriting env, wait, and read result.
     - Add logs: `cli_editor_resolved`, `cli_editor_launch`, `cli_task_body_ready(len)`.

4. [x] New flow: resolve agent, collect body, call RPCs
   - In `new_task()`:
     - Load config via `agency_core::config::load(Some(&root))`.
     - Resolve agent: flag OR `cfg.default_agent` OR error with actionable message.
     - Build `body` from `--message` or `edit_text("")` when interactive.
     - Pass `body` in `TaskNewParams` and selected agent.
     - Call `task.new` and `task.start` (unless `draft`).

5. [ ] Auto-attach, injection, and interactive handoff
   - Extracted helper `attach_and_maybe_inject(...) -> Result<()>` exists and performs attach + optional injection + short read.
   - Update `new_task()` behavior when not draft and not `--no-attach`:
     - Compute prompt: `format!("# Task: {}\n\n{}", info.slug, body)`.
     - If agent is `opencode`, build injection bytes with here-doc:
       - `opencode --agent plan -p "$(cat <<'EOF'\n...\nEOF\n)"`.
     - Attach to the PTY, send the optional injection, and then hand over to `attach_interactive()` to keep the user attached.
     - Ensure the interactive loop prints the detach hint and respects `pty.detach_keys`.
   - Add/adjust a CLI test that verifies: `new --agent opencode -m "Body" feat-xyz` stays attached (TTY), shows the detach hint, and that replay includes agent output when re-attached.

6. [x] Logs and safety
   - Ensure logs are added at key points with event names:
     - `cli_agent_resolved`, `cli_new_autostart_attach`, `cli_new_agent_inject(bytes)`, `cli_editor_resolved`, `cli_editor_launch`, `cli_task_body_ready`.
   - Do not log env values or prompt content.

7. [x] Tests (TDD)
   - Update CLI tests that previously ran `new` to use `--agent fake` and `--no-attach` as needed.
   - Added tests:
     - Requires agent when no config and no `--agent`.
     - `new --agent fake --no-attach -m "Body" feat-abc` writes body to task file and prints running status.
     - Help snapshot unchanged; no update needed.
   - Ensure no tests rely on OpenCode binary.
   - Added assertions for actionable error and body persistence.

8. [ ] Docs
   - Update `README.md` with:
     - `default_agent` behavior and `--agent` requirement when unset.
     - Editor flow (`$EDITOR` -> `vi`) and `--message`.
     - Auto-attach default and `--no-attach`.
     - OpenCode integration prompt format.
     - Environment inheritance guarantees.
     - Clarify that `agency new` keeps you attached interactively by default (equivalent to running `agency attach` afterward).
   - Add notes to this plan during implementation if adjustments occur.

## Notes

- Config `default_agent` is implemented with precedence (project overrides global), and `write_default_project_config()` appends a commented example. Unit tests cover precedence.
- CLI args updated: `--agent` is optional (resolved from config), `--no-attach` and `-m/--message` exist. Top-level help snapshot did not change; subcommand help is not snapshotted.
- Editor helper is implemented and used when interactive. Users can bypass the editor by providing `--message`.
- Current code performs a non-interactive auto-attach (for optional injection) and then detaches. The target behavior for this plan is to keep the user attached interactively after `new` (unless `--no-attach`).
- Environment inheritance aligns with goals: the daemon is spawned from the CLI process and inherits the environment. We explicitly set `AGENCY_SOCKET` and `AGENCY_RESUME_ROOT` for coordination. No environment state is persisted to disk.
- Tests for the actionable "no agent specified" error and for body persistence on `--message` exist and pass without any OpenCode binary on PATH.
- Logging hygiene: only sizes/flags are logged; editor contents and prompt text are not logged.
