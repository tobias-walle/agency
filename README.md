![Agency Logo](./media/logo.svg)

# Agency

> [!CAUTION]
> Agency is in early development and currently does not work. Expect breaking changes and incomplete features.

Agency lets you run multiple AI CLI agents in parallel, each in its own isolated Git worktree.
It uses a single Rust daemon with JSON-RPC and MCP interfaces to manage everything.
Agency is designed for predictable task management, easy TUI attach/detach, and minimal overhead.

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

# Show help / version
agency --help
agency --version

# Daemon lifecycle (macOS launch agent is optional)
agency daemon install
agency daemon start
agency daemon status
# agency daemon stop

# Create and manage tasks
agency status
agency new <slug>
agency start <id|slug>
agency stop <id|slug>
agency attach <id|slug>

# Merge workflow
agency merge <id|slug> [--into <branch>]

# Cleanup merged tasks
agency gc
```

## Configuration

- Global: `~/.config/agency/agency.toml`
- Project: `./.agency/agency.toml`
- Precedence: repository defaults < global (XDG) < project
- Default file: `crates/agency/defaults/agency.toml`


- Global: `~/.config/agency/agency.toml`
- Project: `./.agency/agency.toml`
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
