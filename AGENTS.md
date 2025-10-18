# Agency

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

## Tech Stack

- Rust >=1.89 (workspace uses Edition 2024)
- macOS and Linux supported (Windows not supported)

## Structure

- `./docs/prd/PRD-[id]-[slug].md` - Store for PRDs (Product Requirement Documents). Increment the id when creating a new PRD.
- `./docs/adr/ADR-[id]-[slug].md` - Architecture Decision Records. Increment ids here as well.
- `./docs/plans/PLN-[id]-[slug].md` - High-level plans with self-contained phases (< 0.5 day by a skilled engineer).
- `./docs/guides/*.md` - Contains more detailed documentation in addition to the README.
- `./justfile` - Project scripts
- Entrypoint: `apps/agency/src/main.rs`
- CLI: `crates/cli` (args, RPC client, interactive attach)
- Core: `crates/core` (adapters: fs/git/pty; config; daemon; domain; logging; rpc DTOs)
- Test helpers: `crates/test-support`

## Architecture Overview

- Single binary. The `agency` binary acts as CLI and starts the daemon via `agency daemon run`.
- Transport. JSON-RPC 2.0 over a Unix domain socket (HTTP/1.1 over UDS with `hyper` + `hyperlocal`).
- Core responsibilities.
  - `adapters::fs`: `.agency` layout paths and helpers
  - `adapters::git`: worktrees and branch helpers
  - `adapters::pty`: PTY lifecycle and attach model
  - `config`: defaults, load/merge global + project, env fallbacks
  - `daemon`: JSON-RPC server and method handlers
  - `domain::task`: task model, file format, transitions
  - `logging`: `tracing` JSON logs to `./.agency/logs.jsonl`
  - `rpc`: DTOs for JSON-RPC parameters/results
- CLI responsibilities.
  - Argument parsing, user workflows, autostarting daemon when needed
  - RPC client wrappers and interactive PTY attach loop

## Environment & Configuration

- Precedence: defaults < global config `~/.config/agency/config.toml` < project `./.agency/config.toml`.
- Environment variables:
  - `AGENCY_SOCKET` - absolute path to the Unix socket used by CLI/daemon.
  - `AGENCY_RESUME_ROOT` - when set for the daemon, scan `./.agency/tasks` under that root and resume tasks with `status: running`.
  - `AGENCY_DETACH_KEYS` - override detach sequence shown/used by `agency attach` (e.g. `ctrl-q` or `ctrl-p,ctrl-q`).
- Config fields (see `crates/core/src/config/mod.rs`):
  - `log_level` (off|warn|info|debug|trace; default `info`)
  - `idle_timeout_secs` (default 10)
  - `dwell_secs` (default 2)
  - `concurrency` (None = unlimited)
  - `confirm_by_default` (default false)
  - `[pty].detach_keys` (optional string of comma-separated control keys)

## JSON-RPC Surface (implemented)

- Daemon
  - `daemon.status` -> `{ version, pid, socket_path }`
  - `daemon.shutdown` -> `true`
- Tasks
  - `task.new` -> `{ id, slug, status }` (writes `./.agency/tasks/{id}-{slug}.md`)
  - `task.status` -> `{ tasks: [{ id, slug, status }, ...] }`
  - `task.start` -> `{ id, slug, status }` (ensures branch/worktree, transitions to Running, spawns PTY)
- PTY
  - `pty.attach` -> `{ attachment_id }` (requires task Running, applies initial size)
  - `pty.read` -> `{ data, eof }` (optional `max_bytes`, `wait_ms`)
  - `pty.tick` -> `{ data, eof }` (optional input/resize + read in one call)
  - `pty.input` -> `true`
  - `pty.resize` -> `true`
  - `pty.detach` -> `true`

## Domain & Git Invariants

- Task files: `./.agency/tasks/{id}-{slug}.md` with YAML front matter and Markdown body.
  - Front matter: `base_branch`, `status`, `labels`, `created_at`, `agent`, optional `session_id`.
- Filename regex: `^(\d+)-([A-Za-z0-9-]+)\.md$`.
- Status transitions (enforced): Draft->Running; Running<->Idle; Running->Completed/Failed/Reviewed; Completed/Reviewed->Merged.
- Git branch: `agency/{id}-{slug}`.
- Worktree path: `./.agency/worktrees/{id}-{slug}`.
- Base tip resolution: local `refs/heads/{branch}`; else `refs/remotes/origin/{branch}`; else error.

## PTY Model

- Single active attachment per task session (second attach fails).
- Attach pre‑fills the outbox with the last 128 KiB of history for context replay.
- `pty.read` supports long‑polling (`wait_ms`), avoids busy loops.
- Resize is non‑consuming; `pty.tick` allows batching input + resize + read.
- Detach clears the outbox and unblocks waiters.

## Logging

- Structured JSON logs via `tracing` written to `./.agency/logs.jsonl`.
- Initialized early in `apps/agency/src/main.rs`; helper path in `adapters::fs::logs_path()`.
- Non‑blocking async writer via `tracing_appender`; format includes timestamp, level, and fields.

## CLI Commands (implemented)

- `daemon status|start|stop|run|restart`
- `init`
- `new`
- `start`
- `status`
- `attach`
- `path`
- `shell-hook`

Note: README may include future commands; this file reflects the implemented surface for planning and execution.

## Justfile

All common scripts live in `./justfile`.

Available recipes:

- `setup` - `cargo check`
- `agency *ARGS` - `cargo run -p agency -- {ARGS}`
- `test *ARGS` - `cargo nextest run {ARGS}`
- `check` - `cargo clippy --tests`
- `fmt` - `cargo fmt --all`
- `fix` - `cargo clippy --allow-dirty --allow-staged --tests --fix` then `just fmt`

## Context7 Library IDs

Always look up APIs before you use them and verify usage against the official docs.
Delegate these research tasks to the `api-docs-expert` agent. Give them all the relevant Context7 ids defined below.
If you add a new dependency, resolve its Context7 ID and append it here.

- chrono -> /chronotope/chrono
- dirs -> /dirs-dev/dirs-rs
- regex -> /rust-lang/regex
- serde -> /serde-rs/serde
- serde_yaml -> /dtolnay/serde-yaml
- thiserror -> /dtolnay/thiserror
- toml -> /toml-rs/toml
- tracing -> /tokio-rs/tracing
- tracing-appender -> /tokio-rs/tracing (subcrate)
- tracing-subscriber -> /tokio-rs/tracing (subcrate)
- clap -> /clap-rs/clap
- git2 -> /rust-lang/git2-rs
- tempfile -> /Stebalien/tempfile
- assert_cmd -> /assert-rs/assert_cmd
- pretty_assertions -> /colin-kiegel/rust-pretty-assertions
- proptest -> /proptest-rs/proptest
- serde_json -> /serde-rs/json
- bytes -> /tokio-rs/bytes
- http-body-util -> /hyperium/http-body (subcrate)
- hyper -> /hyperium/hyper
- hyper-util -> /hyperium/hyper-util
- hyperlocal -> /softprops/hyperlocal
- tokio -> /tokio-rs/tokio
- jsonrpsee -> /paritytech/jsonrpsee
- crossterm -> /crossterm-rs/crossterm
- anyhow -> /dtolnay/anyhow
- uuid -> /uuid-rs/uuid
- nix -> /nix-rust/nix
- once_cell -> /matklad/once_cell
- portable-pty -> [resolve with api-docs-expert]
- yansi -> [resolve with api-docs-expert]

## Rules

- Indent code always with 2 spaces
- When committing, follow the conventional commits format
- Prefer ASCII punctuation in docs and code. Avoid long dashes (—) and Unicode arrows (→, ↔); use `-`, `->`, `<->` instead.
- Only add dependencies via `cargo add [pkg]` (exception: dependency already exists). Never modify Cargo.toml directly.
- Make use of subagents via the `task` tool to keep the context concise
- Use the `api-docs-expert` subagent when working with libraries
  - Lookup new APIs before you use them
  - Check correct API use when encountering errors
- Never use single letter variable names if they span more than 3 lines
- You SHOULD use TDD then appropriate:
  - Fixing bugs -> Write tests before implementation
  - Implement new features, with unclear final solution -> Write tests after implementation
- Before writing or editing Rust code, you MUST read `./docs/guides/RUST_BEST_PRACTICES.md` and follow it

## Testing

- Keep tests readable and focused on behavior.
- Highly emphasize actionable assertion output (what, why, actual vs expected).
- Centralize setup in `crates/test-support` (daemon, RPC, CLI, git, PTY helpers).
- Prefer polling with bounded timeouts over fixed sleeps to reduce flakiness.
- Use `git2` for local repositories instead of shelling out to `git`.
- Avoid global env mutations; prefer per-command `.env()` or scoped guards.
- Use consistent file names (feature-oriented) and `subject_action_expected` test names.
