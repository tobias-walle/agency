![Agency Logo](./media/logo.svg)

# Agency

Agency orchestrates command-line AI agents across isolated Git worktrees.

- Git like cli interface (see `agency --help`)
- Terminal UI (`agency tui`) similar to lazygit
- Automatic creation of worktrees and background process management

## Requirements

- Rust >= 1.89
- macOS or Linux (Windows is not supported)

## Install

Install the CLI from source:

```bash
just install-globally
```

## First Run

```bash
# Run the wizard (auto-runs if you call `agency` without a config in place)
agency setup

# Scaffold project-local overrides and a bootstrap script
agency init

# Create your first task (opens the markdown and attaches by default)
agency new implement-cool-feature

# Launch the TUI (same as running `agency` with no arguments once configured)
agency tui
```

`agency new` creates `.agency/tasks/<id>-<slug>.md`, records the current branch as the base, and starts+attaches to the configured agent through the daemon unless you pass `--draft`.

## Task Workflow

- `agency ps` - list tasks, worktree status, latest session id, base branch, and effective agent.
- `agency attach <id|slug>` - attach to an already running task session.
- `agency start <id|slug>` - prepare the branch/worktree, run bootstrap, start the session and attach; fails if already started.
- `agency sessions` - show all live sessions for the current repo; reuse ids with `agency attach --session <id>` or `agency stop --session <id>`.
- `agency stop [<id|slug>] [--session <id>]` - terminate sessions cleanly.
- `agency merge <id|slug> [--branch main]` - fast-forward the worktree branch into the base and clean up branch plus worktree when complete.
- `agency bootstrap <id|slug>` - rerun the worktree bootstrap flow without creating a session.
- `agency open|edit|path|branch <id|slug>` - open the worktree directory, open the task markdown, print the worktree path, or print the branch name.
- `agency rm <id|slug>` and `agency reset <id|slug>` - delete the task (including worktree and branch) or reset the worktree while keeping the markdown.

The TUI mirrors these actions with keybindings (arrows/j/k to navigate, Enter to edit/attach, `n` to create, `s` to start, `S` to stop, `m` to merge, `X` to delete, `ctrl-q` to detach by default). Logs from CLI commands stream into the bottom panel so you can monitor bootstrap scripts and daemon events.

## Daemon and Sessions

- `agency daemon start|stop|restart` - manage the background PTY daemon manually. `agency attach` and `agency new` start it automatically when needed.
- Sessions are keyed by repository root and run behind a Unix socket derived from config (`daemon.socket_path` overrides the `$XDG_RUNTIME_DIR/agency.sock` fallback).
- Agent commands run inside task worktrees with `$AGENCY_TASK` set to the task markdown body (front matter and title stripped) so agents can read the prompt text directly.

## Configuration

Configuration is layered in three tiers:

1. Embedded defaults (`crates/agency/defaults/agency.toml`)
2. Global file `~/.config/agency/agency.toml` (created by `agency setup`)
3. Project overrides at `./.agency/agency.toml`

Inspect the read-only defaults with:

```bash
agency defaults
```

Key sections you can override:

- `agent` - default agent when a task lacks front matter overrides.
- `[agents.<name>.cmd]` - argv template for launching an agent. Tokens expand `$VARS` using the session environment (including `AGENCY_TASK`) and replace `<root>` with the repository root.
- `[bootstrap]` - include/exclude lists that control which files or directories are copied into new worktrees and an optional `cmd` to run (defaults to `<root>/.agency/setup.sh`).
- `[keybindings]` - customize TUI shortcuts (the setup wizard lets you change the `detach` binding up front).

Tasks can declare their own agent or base branch in YAML front matter:

```markdown
---
agent: claude
base_branch: main
---

# Implement cool feature
```

## Bootstrapping Worktrees

When a task session starts or you run `agency bootstrap`, Agency:

1. Ensures `.agency/worktrees/<id>-<slug>` exists via `git worktree add`.
2. Copies git-ignored files (up to 10 MB each) that match `bootstrap.include` while respecting exclusions.
3. Clones any directories listed in `bootstrap.include` (skipping `.git`, `.agency`, and entries in `bootstrap.exclude`).
4. Runs the configured bootstrap command in the worktree. The default `<root>/.agency/setup.sh` is skipped silently if the script is missing.

You can rerun the bootstrap step without creating a session via `agency bootstrap <id|slug>`.

## Development

Common Just recipes:

```bash
just check     # Run clippy
just test      # Run the nextest suite
just fmt       # Format the workspace
just agency -- <cmd>  # Run the CLI in development mode
```

The crate targets Rust 1.89+ (edition 2024) and uses `parking_lot`, `ratatui`, `crossterm`, `gix`, and a bespoke PTY daemon (`crates/agency/src/pty`). Browse `docs/plans/` for design history and upcoming work.
