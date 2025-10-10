![Agency Logo](./media/logo.svg)

# Agency

> [!CAUTION]
> Agency is in early development and currently does not work. Expect breaking changes and incomplete features.

Agency lets you run multiple AI CLI agents in parallel, each in its own isolated Git worktree.
It uses a single Rust daemon with JSON-RPC and MCP interfaces to manage everything.
Agency is designed for predictable task management, easy TUI attach/detach, and minimal overhead.

## Features

- Git worktree isolation with per-task branches
- Single user daemon exposing JSON-RPC 2.0 over a Unix domain socket
- MCP server bridging to the daemon task API
- Reliable PTY backend for TUIs (e.g., Opencode, Neovim) with attach/detach and resize handling
- Structured JSON logging with configurable verbosity
- Global and per-project configuration

## Requirements

- Rust >= 1.89
- macOS and Linux supported (Windows not supported)

## Installation

Build the workspace:

```bash
cargo build --workspace
```

Or run the app directly:

```bash
just start
```

## Quickstart

Basic CLI usage:

```bash
# Initialize project scaffolding and config
agency init

# Daemon lifecycle (macOS launch agent is optional)
agency daemon install
agency daemon start
agency daemon status
# agency daemon stop

# Create and manage tasks
agency new <slug>
agency edit <id|slug>
agency start <id|slug>
agency stop <id|slug>
agency attach <id|slug>
agency idle <id|slug>
agency complete <id|slug>
agency fail <id|slug>
agency reviewed <id|slug>
agency status

# Merge workflow
agency merge <id|slug> [--into <branch>]

# Cleanup merged tasks
agency gc

# Utilities
agency path <id|slug>
agency shell-hook
agency session set <id|slug> <session_id>
```

Agency spawns the selected agent directly inside the task PTY when a task starts.
`agency new` auto-attaches only when stdout is a TTY (unless `--no-attach`); non-interactive runs print the task status and return immediately.

Helpful commands:

```bash
# Show help / version
agency --help
agency --version

# Run tests / checks
just test
just check
```

## Task Lifecycle

- `Running`: task process is active and attachable.
- `Stopped`: daemon restarted a previously running task; run `agency start` to launch the agent again.
- `Idle`: user paused the task without terminating the process.

When the daemon restarts it marks every `Running` task as `Stopped` on disk and leaves the PTY offline until the task is started again.

## Configuration

- Global: `~/.config/agency/config.toml`
- Project: `./.agency/config.toml`
- Socket: `AGENCY_SOCKET` controls the Unix socket path
- Selected settings: log level (off|warn|info|debug|trace), idle timeout, concurrency limits, confirmation defaults

Agent launch commands live under `[agents.<name>]` with per-action arrays.
If an agent is missing or its `start` command is empty the daemon returns an actionable error.
The daemon ships defaults for `opencode` and `fake`, and you can override or extend them per project.

```toml
[agents.opencode]
display_name = "OpenCode"
start = ["opencode", "--agent", "plan", "-p", "$AGENCY_PROMPT"]

[agents.fake]
display_name = "Shell"
start = ["sh"]
```

Arguments support `$AGENCY_*` placeholders which are expanded before the process starts.
The same keys (task id, slug, body, prompt, project root, worktree, optional session/message) are also exported as environment variables in the child process.

## Logging

- Structured JSON logs are written to `./.agency/logs.jsonl`
- Each entry includes timestamp, level, task id/slug (when applicable), and event context

## License

MIT — see `LICENSE.md`.
