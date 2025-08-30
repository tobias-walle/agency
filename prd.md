# Orchestra PRD (v1)

The Orchestra tool orchestrates parallel-running AI CLI agents in isolated Git worktrees, with a single Rust daemon, a JSON-RPC API, an MCP interface, and a thin CLI. The design optimizes for deterministic task lifecycles, ergonomic TUI attach/detach, and low overhead.

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

- Task files live at `./.orchestra/tasks/{id}:{slug}.md`
- `id` (numeric, autoincrement) and `slug` (string) are parsed from the filename
- YAML header is the only persisted metadata; do not duplicate computable or ephemeral fields

YAML fields:

```markdown
---
title: <string>
base_branch: <string>      # e.g. main
status: <enum>             # draft | running | idle | completed | reviewed | failed | merged
labels: [<string>]
created_at: <ISO8601>
agent: <enum>              # opencode | claude-code | custom
session_id: <string|null>  # updated if the tool starts a new session (/new, /clear)
---
<freeform markdown description>
```

Derived (not stored):
- worktree path: `./.orchestra/worktrees/{id}-{slug}`
- branch name: `orchestra/task-{id}-{slug}`
- base branch head SHA at start is captured in memory/events

### Status Lifecycle

- draft → running → idle ↔ running → completed/reviewed/failed → merged
- Remove `paused`. `failed` can be set by MCP/CLI.
- `merged` is set after a successful merge; `gc` deletes merged tasks on request.

Transitions:
- new task: `draft`
- start: allocate worktree/branch, run setup, spawn agent in PTY, set `running`
- idle detection: if no PTY output for 10s (configurable), set `idle`; any output or user keystroke flips back to `running` (with 2s dwell to avoid flapping)
- complete/review/fail: explicit user or adapter/agent signals
- merge: allowed only from `completed` or `reviewed`; on success set `merged` and clean up

## Architecture

### Daemon

- Single user-level daemon written in Rust
- Exposes JSON-RPC 2.0 over a Unix domain socket
- Socket path controlled by `ORCHESTRA_SOCKET`
  - Linux default: `$XDG_RUNTIME_DIR/orchestra.sock` or fallback `/tmp/orchestra.sock`
  - macOS default: `$TMPDIR/orchestra.sock`
- PID and in-memory state (process handles, PTY sessions) managed internally; only one instance runs (file lock on socket path)
- `orchestra install` optionally sets up a launchd agent (macOS) with user confirmation

### PTY Backend

- Implemented via `portable-pty`
- Supports TUIs (Opencode, Neovim) reliably
- Propagate terminal size on attach and handle resize events
- Single active attachment at a time (v1)

### Git Integration

- Use `git2` for worktrees/branches
- Worktree: `./.orchestra/worktrees/{id}-{slug}`
- Branch: `orchestra/task-{id}-{slug}`
- On start: ensure `base_branch` exists and is up to date; record base tip SHA in events/in-memory
- After merge: remove worktree and optionally delete local task branch; mark task `merged`

### Interfaces

- CLI (via `clap`) acts as a JSON-RPC client to the daemon
- MCP server exposed by `orchestra mcp` subcommand, bridging to the daemon task API

### Configuration

- Global: `~/.config/orchestra/config.yaml`
- Project: `./.orchestra/config.yaml`
- Settings include: log level (off|warn|info|debug|trace), idle timeout (default 10s), concurrency limits, confirmation policy defaults

### Setup Script

- Optional per-project `setup` script executed in the new worktree before agent start
- Receives env: `ORCHESTRA_TASK_ID`, `ORCHESTRA_TASK_SLUG`, `ORCHESTRA_WORKTREE_PATH`, `ORCHESTRA_BASE_BRANCH`, `ORCHESTRA_BRANCH_NAME`, `ORCHESTRA_AGENT`
- Non-zero exit marks task `failed` and logs output

## Logging and Observability

- Structured JSON logs via `tracing` written to `./.orchestra/logs.jsonl`
- Each log entry includes timestamp, level, task id/slug (when applicable), and event context
- Log level configured globally or per project; verbose logging opt-in
- Event timeline per task appended to the central log stream (no per-task files in v1)
- `orchestra status --json` for machine-readable state, with `--watch` for streaming

## Agents and Adapters

- Built-in adapters: `opencode` (v1), `claude-code` (v1/1.1), and `fake` for testing
- Adapter trait:
  - prepare(env, cwd) -> Result<()> (optional)
  - start(task, ctx) -> spawn command under PTY; return handle bound to daemon state
  - stop(task) -> graceful then kill with timeout
  - parse_session_update(output_chunk) -> Option<session_id>
- `session_id` is updated if tools emit a new session token; fallback CLI: `orchestra session set <id|slug> <session_id>`

## CLI Commands

- `orchestra init`                   # create project scaffolding and config
- `orchestra install`                # interactive setup of launch agent (macOS)
- `orchestra daemon start|status|stop`
- `orchestra new [slug]`             # creates task file, opens $EDITOR to set title/body
- `orchestra edit <id|slug>`
- `orchestra start <id|slug>`
- `orchestra stop <id|slug>`         # confirm unless `-y`
- `orchestra attach <id|slug>`
- `orchestra complete <id|slug>`
- `orchestra fail <id|slug>`
- `orchestra review <id|slug>`
- `orchestra status [--json]`
- `orchestra merge <id|slug> [--into <branch>]`  # confirm unless `-y`
- `orchestra gc`                     # deletes tasks in `merged` state (confirm unless `-y`)
- `orchestra path <id|slug>`         # prints worktree path
- `orchestra shell-hook`             # prints shell function to `cd` into worktree (zsh/bash/fish)
- `orchestra config get|set <key> [value]`
- `orchestra session set <id|slug> <session_id>`

All destructive commands prompt for confirmation by default; `-y` overrides. Defaults configurable.

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

## Implementation Stack (Rust)

- Async runtime: `tokio`
- PTY: `portable-pty`
- JSON-RPC over Unix sockets: `jsonrpsee` (or similar)
- CLI: `clap`
- Git: `git2`
- YAML/JSON: `serde`, `serde_yaml`, `serde_json`
- Logging: `tracing`, JSON formatter
- Filesystem utils: `tokio::fs`, `tempfile` for tests

## Acceptance Criteria

- Create, start, attach to, and complete a task in a temp repo with Opencode and the fake agent
- Idle flips after 10s of no PTY output and returns to running on new output
- Merge only allowed from completed/reviewed; marks task as merged; cleans worktree and local branch
- `gc` removes merged tasks after confirmation
- Single daemon instance enforces socket lock; configurable via `ORCHESTRA_SOCKET`
- Logs written to `./.orchestra/logs.jsonl` with structured fields and configurable levels
- TUI agents render correctly via PTY; attach/detach preserves state; resize works
- CLI provides JSON output for status and returns non-zero on errors
