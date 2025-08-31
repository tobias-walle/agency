# Orchestra

Orchestra orchestrates parallel-running AI CLI agents in isolated Git worktrees, powered by a single Rust daemon with JSON-RPC and MCP interfaces. It optimizes for deterministic task lifecycles, ergonomic TUI attach/detach, and low overhead.

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
orchestra init

# Daemon lifecycle (macOS launch agent is optional)
orchestra daemon install
orchestra daemon start
orchestra daemon status
# orchestra daemon stop

# Create and manage tasks
orchestra new <slug>
orchestra edit <id|slug>
orchestra start <id|slug>
orchestra stop <id|slug>
orchestra attach <id|slug>
orchestra idle <id|slug>
orchestra complete <id|slug>
orchestra fail <id|slug>
orchestra reviewed <id|slug>
orchestra status

# Merge workflow
gorchestra merge <id|slug> [--into <branch>]

# Cleanup merged tasks
gorchestra gc

# Utilities
gorchestra path <id|slug>
orchestra shell-hook
orchestra session set <id|slug> <session_id>
```

Helpful commands:

```bash
# Show help / version
orchestra --help
orchestra --version

# Run tests / checks
just test
just check
```

## Configuration

- Global: `~/.config/orchestra/config.toml`
- Project: `./.orchestra/config.toml`
- Socket: `ORCHESTRA_SOCKET` controls the Unix socket path
- Selected settings: log level (off|warn|info|debug|trace), idle timeout, concurrency limits, confirmation defaults

## Logging

- Structured JSON logs are written to `./.orchestra/logs.jsonl`
- Each entry includes timestamp, level, task id/slug (when applicable), and event context

## License

MIT — see `LICENSE.md`.
