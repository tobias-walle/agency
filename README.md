# Agency

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

Helpful commands:

```bash
# Show help / version
agency --help
agency --version

# Run tests / checks
just test
just check
```

## Configuration

- Global: `~/.config/agency/config.toml`
- Project: `./.agency/config.toml`
- Socket: `AGENCY_SOCKET` controls the Unix socket path
- Selected settings: log level (off|warn|info|debug|trace), idle timeout, concurrency limits, confirmation defaults

## Logging

- Structured JSON logs are written to `./.agency/logs.jsonl`
- Each entry includes timestamp, level, task id/slug (when applicable), and event context

## License

MIT — see `LICENSE.md`.
