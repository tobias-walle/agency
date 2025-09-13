# PLN-10: Integrate OpenCode Agent, Editor Descriptions, Auto-Attach, Env Inheritance

Date: 2025-09-13

Add agent execution (OpenCode), editor-based task descriptions, auto-attach after creation, and centralized environment inheritance.

## Goals

- Enable running an external coding agent (OpenCode) for tasks.
- Allow providing a task description via `$EDITOR` or `-m,--message`.
- Auto-attach to the task PTY after creation (opt-out via `--no-attach`).
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
  - `NewArgs` requires `agent` with default `fake`; no description input.
- CLI flows are in `crates/cli/src/lib.rs`.
  - `new_task()` calls `task.new` with `body: None`, then immediately `task.start`, and does not attach or run agents.
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
  - No `default_agent` exists; `pty.detach_keys` is present.
- There is no editor flow; users cannot provide a task description interactively.

## Solution

- Add `default_agent: Option<Agent>` to configuration (global and project) with standard precedence.
- Make `--agent` optional for `agency new`; resolve agent from flag or config, else error.
- Add description capture before `task.new` via `$EDITOR` -> `vi`, or `-m,--message` to bypass editor.
- Pass description as `body` in `TaskNewParams`.
- After successful `task.start`, auto-attach unless `--no-attach`.
- If agent is `opencode`, inject a one-shot shell command into the PTY to run `opencode --agent plan -p "<prompt>"`, where `<prompt>` is `# Task: [slug]\n\n[description]` using a here-doc for safe multiline quoting.
- Centralize environment behavior by relying solely on process inheritance; do not store env (Note: The env needs to be inherited by the cli, not the daemon. An exception is then the task was started with `daemon (re)start`)
- Add tracing logs around editor resolution, agent resolution, attach, and injection (avoid logging env contents or prompt text; log sizes/flags only).
- Keep tests using `fake` agent and `--no-attach` where needed to avoid OpenCode dependency.

## Detailed Plan

HINT: Update checkboxes during the implementation

1. [ ] Config: add `default_agent`
   - Update `crates/core/src/config/mod.rs`:
     - Add `pub default_agent: Option<crate::domain::task::Agent>` to `Config`.
     - Add to `PartialConfig` and merge logic.
     - Update `Default` and `write_default_project_config()` to include commented example:
       - `# default_agent = "opencode" | "claude-code" | "fake"`.
   - Add/adjust unit tests for config merge and defaults.

2. [ ] CLI args: make agent optional, add message/no-attach
   - Update `crates/cli/src/args.rs`:
     - Change `NewArgs.agent` to `Option<AgentArg>`.
     - Add `#[arg(long = "no-attach")] pub no_attach: bool`.
     - Add `#[arg(short = 'm', long = "message")] pub message: Option<String>`.
   - Update help snapshot tests if output changes.

3. [ ] Editor helper
   - In `crates/cli/src/lib.rs`, add `fn edit_text(initial: &str) -> anyhow::Result<String>`:
     - Resolve editor from `$EDITOR` or fallback `vi`.
     - Write `initial` to a temp file, spawn editor inheriting env, wait, and read result.
     - Add logs: `cli_editor_resolved`, `cli_editor_launch`, `cli_task_body_ready(len)`.

4. [ ] New flow: resolve agent, collect body, call RPCs
   - In `new_task()`:
     - Load config via `agency_core::config::load(Some(&root))`.
     - Resolve agent: flag OR `cfg.default_agent` OR error with actionable message.
     - Build `body` from `--message` or `edit_text("")`.
     - Pass `body` in `TaskNewParams` and selected agent.
     - Call `task.new` and `task.start` (unless `draft`).

5. [ ] Auto-attach and injection helper
   - Extract a helper `attach_and_maybe_inject(...) -> Result<()>` in `crates/cli/src/lib.rs`:
     - Attach using `pty_attach_with_replay`.
     - If `initial_input` is `Some`, send via `rpc::client::pty_input` before entering the interactive loop.
     - Use the existing interactive loop when needed.
   - In `new_task()`, if not draft and not `--no-attach`:
     - Compute prompt: `format!("# Task: {}\n\n{}", info.slug, body)`.
     - If agent is `opencode`, build injection bytes:
       - `b"opencode --agent plan -p \"$(cat <<'EOF'\n...\nEOF\n)\"\n"`.
     - Call helper with `initial_input = Some(bytes)`.

6. [ ] Logs and safety
   - Ensure logs are added at key points with event names:
     - `cli_agent_resolved`, `cli_new_autostart_attach`, `cli_new_agent_inject(bytes)`.
   - Do not log env values or prompt content.

7. [ ] Tests (TDD)
   - Update CLI tests that previously ran `new` to use `--agent fake` and `--no-attach` as needed.
   - Add tests:
     - Requires agent when no config and no `--agent`.
     - `new --agent fake --no-attach -m "Body" feat-abc` writes body to task file and prints running status.
     - Help snapshot updated if CLI help changed.
   - Ensure no tests rely on OpenCode binary.

8. [ ] Docs
   - Update `README.md` with:
     - `default_agent` behavior and `--agent` requirement when unset.
     - Editor flow (`$EDITOR` -> `vi`) and `--message`.
     - Auto-attach default and `--no-attach`.
     - OpenCode integration prompt format.
     - Environment inheritance guarantees.
   - Add notes to this plan during implementation if adjustments occur.

## Notes

HINT: Update this section during the implementation with relevant changes to the plan, problems that arised or other noteworthy things.

- We intentionally avoid persisting or copying env; all processes inherit from their parent at spawn time.
- Agent injection is a single command sent into the PTY; future enhancements might include richer lifecycle management.
- Tests prefer `fake` agent to remain hermetic and avoid depending on OpenCode.
