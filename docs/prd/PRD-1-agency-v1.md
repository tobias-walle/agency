# Agency PRD (v1)

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees, with a single Rust daemon, a JSON-RPC API, an MCP interface, and a thin CLI. The design optimizes for deterministic task lifecycles, ergonomic TUI attach/detach, and low overhead.

## Goals

- Streamline creation, execution, and review of agent-driven tasks per project
- Provide robust isolation via Git worktrees/branches
- Support interactive TUI agents (e.g., Opencode, Neovim) via a single PTY backend
- Offer a single user daemon with JSON-RPC and MCP interfaces
- Keep logs structured and centralized, configurable for verbosity
- Favor Rust for performance, correctness, and portability

## Non-Goals (v1)

- No web UI
- No sandboxing or process isolation beyond worktree
- No multiple session backends (no tmux dependency)

## Core Concepts

### Tasks

- Task files live at `./.agency/tasks/{id}-{slug}.md`
- `id` (numeric, autoincrement) and `slug` (string) are parsed from the filename
- YAML header is the only persisted metadata; do not duplicate computable or ephemeral fields

YAML fields:

```markdown
---
base_branch: <string> # e.g. main
status: <enum> # draft | running | idle | completed | reviewed | failed | merged
labels: [<string>]
created_at: <ISO8601>
agent: <enum> # opencode | claude-code | fake
session_id: <string|null> # set initially; updated if the tool starts a new session (e.g., /new, /clear)
---

<freeform markdown description>
```

Derived (not stored):

- worktree path: `./.agency/worktrees/{id}-{slug}`
- branch name: `agency/{id}-{slug}`
- Base branch head SHA is captured when the task is started and recorded in the central event log as `base_sha` (not persisted in YAML). This value is used for diff computation and merge validation.

### Status Lifecycle

- draft → running → idle ↔ running → completed/failed/reviewed → merged
- `failed` and `completed` can be set by MCP/CLI.
- `reviewed` can be set from the user with the CLI.
- `merged` is set after a successful merge
- `gc` deletes merged tasks on request.

Transitions:

- new task: `draft`
- start: allocate worktree/branch, run setup, spawn agent in PTY, set `running`
- idle detection: if no PTY output for 10s (configurable), set `idle`; any output or user keystroke flips back to `running` (with 2s dwell to avoid flapping). The idle state can also be triggered via the CLI `agency idle <session-id|id|slug>` (via adapter hooks)
- complete/reviewed/fail: explicit user or adapter/agent signals
- merge: allowed only from `completed` or `reviewed`; on success set `merged` (cleanup needs to be triggered via gc)

## Architecture

### Daemon

- Single user-level daemon written in Rust
- Exposes JSON-RPC 2.0 over a Unix domain socket
- Socket path controlled by `AGENCY_SOCKET`
  - Linux default: `$XDG_RUNTIME_DIR/agency.sock` or fallback `/var/run/agency.sock`
  - macOS default: `$XDG_RUNTIME_DIR/agency.sock` or fallback `/Library/Application Support/agency/agency.sock`
  - Windows: Not supported (return error)
- PID and in-memory state (process handles, PTY sessions) managed internally; only one instance runs (file lock on socket path)
- `agency daemon install` optionally sets up a launchd agent (macOS) with user confirmation

### PTY Backend

- Implemented via `portable-pty`
- Supports TUIs (Opencode, Neovim) reliably
- Propagate terminal size on attach and handle resize events
- Single active attachment at a time (v1)
- Detach: default Ctrl-q; configurable via config `pty.detach_keys` or env `AGENCY_DETACH_KEYS` (no CLI flag)
- Do not override Ctrl-C by default; pass through to the PTY app
- On successful attach, print hint: "Attached. Detach: Ctrl-q (configurable)"

### Git Integration

- Use `git2` for worktrees/branches
- Worktree: `./.agency/worktrees/{id}-{slug}`
- Branch: `agency/{id}-{slug}`
- On start: ensure `base_branch` exists and is up to date; record base tip SHA in events/in-memory
- After merge: remove worktree and optionally delete local task branch; mark task `merged`

### Interfaces

- CLI (via `clap`) acts as a JSON-RPC client to the daemon
- MCP server exposed by `agency mcp` subcommand, bridging to the daemon task API (uses <https://github.com/modelcontextprotocol/rust-sdk>)

### Configuration

- Global: `~/.config/agency/config.toml`
- Project: `./.agency/config.toml`
- Settings include: log level (off|warn|info|debug|trace), idle timeout (default 10s), dwell (2s), PTY detach keys (`pty.detach_keys`, default `ctrl-q`), concurrency limits, confirmation policy defaults
- Env override: `AGENCY_DETACH_KEYS` to override detach sequence per-session
- Global and local config are merged

### Setup Script

- Optional per-project `setup` script executed in the new worktree before agent start
- Receives env: `AGENCY_TASK_ID`, `AGENCY_TASK_SLUG`, `AGENCY_WORKTREE_PATH`, `AGENCY_BASE_BRANCH`, `AGENCY_BRANCH_NAME`, `AGENCY_AGENT`
- Non-zero exit marks task `failed` and logs output

## Logging and Observability

- Structured JSON logs via `tracing` written to `./.agency/logs.jsonl`
- Each log entry includes timestamp, level, task id/slug (when applicable), and event context
- Log level configured globally or per project; verbose logging opt-in
- Event timeline per task appended to the central log stream (no per-task files in v1)

## Agents and Adapters

- Initial Adapters: `opencode`, `claude-code`, and `fake` for testing
- Adapters are configured via `.agency/agents/*.toml`.
  Each adapter defines how to spawn the agent or resume the session.

  **Example (`.agency/agents/fake.toml`):**

  ```toml
  [adapter]
  name = "fake"
  cmd = "agency-fake-agent"
  cmd_resume = "agency-fake-agent --resume $AGENCY_SESSION_ID"
  ```

- Include a `fake` adapter for tests.

## CLI Commands

- `agency init` # create project scaffolding and config
- `agency daemon install` # interactive setup of launch agent (macOS)
- `agency daemon start|status|stop`
- `agency new [slug]` # creates task file, opens $EDITOR to set body
- `agency edit <id|slug>`
- `agency start <id|slug>`
- `agency stop <id|slug>` # confirm unless `-y`
- `agency attach <id|slug>` # hint shown on attach; detach configurable via config/env
- `agency idle <id|slug>` # manually set idle state (optional)
- `agency complete <id|slug>`
- `agency fail <id|slug>`
- `agency reviewed <id|slug>`
- `agency status`
- `agency merge <id|slug> [--into <branch>]`
- `agency gc` # deletes tasks in `merged` state (list all tasks to delete and confirm unless `-y`)
- `agency path <id|slug>` # prints worktree path
- `agency shell-hook` # prints shell function to `cd` into worktree (zsh/bash/fish/nushell)
- `agency session set <id|slug> <session_id>`

All destructive commands prompt for confirmation by default; `-y` overrides. Defaults configurable. The default answer is "No" (`confirm_by_default = false`) for safety; users can opt-in to auto-confirm via config or flags.

## Idle Detection

- If no PTY output for 10 seconds (configurable), mark `idle`
- Any PTY output or user keystroke through an attached session returns to `running`
- Apply a 2s dwell time to avoid flapping; suppress duplicate transitions in logs
- Adapters may explicitly signal idle; explicit signals take precedence

## Merge Policy

- Allowed only for tasks in `completed` or `reviewed`
- Default target is `base_branch`; CLI `--into` overrides
- Default strategy: squash (implementation detail; recorded in events, not YAML)
- On success: set status `merged`, remove worktree, delete local task branch
- `gc` removes `merged` task files and any dangling local artifacts

## Confirmation and Safety

- Confirm destructive actions: `stop`, `merge`, `gc` (and any implicit destructive operations)
- `-y` skips prompts; config can set default behavior per environment

## Testing Strategy

- Fake agent in Rust implements adapter trait without calling external AI
- Fast integration tests: temp git repo; exercise `new/start/attach/idle/complete/merge/gc`
- E2E tests: spawn daemon, PTY attach, resize handling, idle transitions, merge flow
- Minimal mocking: PTY and filesystem boundaries only
- Deterministic outputs from fake agent; CI-friendly without network
- Utilities are provided for tempfiles/folders

## Implementation Stack (Rust)

- Async runtime: `tokio`
- PTY: `portable-pty`
- JSON-RPC over Unix sockets: `jsonrpsee`
- CLI: `clap`
- Git: `git2`
- TOML/JSON: `serde`, `toml`, `serde_json`
- Logging: `tracing`, JSON formatter
- Filesystem utils: `tokio::fs`
