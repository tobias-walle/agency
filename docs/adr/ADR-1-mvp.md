# ADR-1: MVP Architecture and Defaults (mvp)

Date: 2025-08-30

## Status

Accepted

## Context

The Orchestra v1 PRD defines a single-user daemon that orchestrates AI CLI agents in isolated Git worktrees with a JSON-RPC API, MCP interface, and a thin CLI.
We want a lean, readable architecture to ship an MVP quickly without over-fragmenting the codebase.

Related PRD: [PRD-1-orchestra-v1.md](../prd/PRD-1-orchestra-v1.md)

## Decision

1. Transport: Use HTTP over Unix Domain Sockets via `hyper` + `hyperlocal` for JSON-RPC (server) in the daemon. Overhead is not a concern at the moment.
2. Packaging: Ship a single binary for all commands (`orchestra`), including the CLI and an `mcp` subcommand that starts the MCP server.
3. CLI Styling: Use `yansi` for colors and `comfy-table` for tabular output.
4. Defaults: Keep PRD defaults for idle detection (10s idle threshold, 2s dwell) and other timeouts.
5. Merge Policy: Strictly squash merges by default with optional `--into` to override target branch.

## Architecture Overview

A single long-running daemon exposes a JSON-RPC 2.0 API over a Unix domain socket.
The daemon manages task lifecycles, Git worktrees/branches, PTY-backed agent sessions, and structured logging.
The `orchestra` binary provides all user commands and also offers an `mcp` subcommand to start the MCP server that bridges to the daemon API.
All orchestration and side effects live in `core`; clients remain thin.

## Workspace Structure (high-level)

```
orchestra/                     # workspace root
  crates/
    core/                      # daemon + domain + adapters (PTY, Git) + config + logging
    cli/                       # thin JSON-RPC client (clap), colorful UX
    mcp/                       # MCP bridge/server translating to daemon API
  apps/
    orchestra/                 # single binary entrypoint (CLI + `mcp` subcommand)
```

Notes:

- Start with max four crates to keep boundaries clear and maintenance simple.
- `core` encapsulates IO, concurrency, and state; `cli` and `mcp` depend on `core` API surface (or JSON-RPC client when appropriate).

## Responsibilities (by crate)

- `core`
  - Domain model: tasks with YAML front matter, statuses, validated transitions.
  - Daemon: single instance (file lock), JSON-RPC server over hyperlocal, in-memory state/supervision.
  - Adapters: Git worktrees/branches (`git2`), PTY management (`portable-pty`), setup script exec.
  - Idle detection with dwell to avoid flapping.
  - Config loading/merging (global + project) and tracing setup (JSON logs to `.orchestra/logs.jsonl`).
- `cli`
  - `clap` commands mapping 1:1 to PRD.
  - JSON-RPC client to daemon.
  - Colorful output (`yansi`), tables (`comfy-table`), confirmations, spinners.
- `mcp`
  - Expose MCP server (Model Context Protocol) that forwards to daemon API.
  - Shares DTOs/serialization with daemon/cli to avoid drift.
- `apps/orchestra`
  - Single binary entrypoint.
  - Dispatch CLI commands; provide `orchestra mcp` to run the MCP server.

## Runtime Flows (concise)

- Daemon startup: ensure socket path from `ORCHESTRA_SOCKET` (platform defaults per PRD), acquire file lock, init tracing and config, start JSON-RPC server.
- Task lifecycle: `new → start → (idle ↔ running) → completed|failed|reviewed → merge → gc` with validations; `stop` allowed with confirmation.
- Start: ensure `base_branch` up to date, record `base_sha`, create worktree + branch, run optional setup script, spawn agent in PTY, set `running`.
- Idle detection: if no PTY output for 10s (configurable), set `idle`; any output/keystroke returns to `running` after 2s dwell.
- Merge: allowed from `completed|reviewed`; squash by default into `base_branch` or `--into` override; on success set `merged`, remove worktree/branch.
- GC: remove `merged` task files and local artifacts after confirmation.

## Interfaces (high-level)

- JSON-RPC 2.0 over HTTP/UDS (hyperlocal). Example method groups:
  - `daemon.*`: `status`.
  - `task.*`: `new`, `edit`, `start`, `stop`, `idle`, `complete`, `fail`, `reviewed`, `merge`, `gc`, `status`, `path`, `session.set`.
  - `pty.*`: `attach` (server-sent stream), `input`, `resize`.
- MCP: methods mirror task/pty operations, acting as a thin bridge to the daemon.

## Configuration & Defaults

- Global config: `~/.config/orchestra/config.toml`; Project config: `./.orchestra/config.toml`; merged with project overriding global.
- Key settings: log level (off|warn|info|debug|trace), idle timeout (10s), dwell (2s), concurrency limits, confirmation defaults.
- Socket path: `ORCHESTRA_SOCKET` with platform-specific defaults from PRD; Windows not supported.

## Logging & Observability

- `tracing` with JSON formatter writes to `./.orchestra/logs.jsonl`.
- All lifecycle transitions and significant side effects emit structured events (timestamp, level, task id/slug, context).

## Dependencies (by crate, latest)

- `core`
  - Runtime: `tokio`
  - RPC (server): `jsonrpsee`, `hyper`, `hyperlocal`
  - PTY: `portable-pty`, `nix`
  - Git: `git2`
  - Serialization/Config: `serde`, `serde_yaml`, `serde_json`, `toml`, `dirs`
  - Logging: `tracing`, `tracing-subscriber`, `tracing-appender`
  - Errors/locking: `thiserror`, `fd-lock`
- `cli`
  - CLI: `clap` (derive)
  - RPC (client): `jsonrpsee`
  - UI: `yansi`, `comfy-table`, `dialoguer`, `indicatif`
  - Serde: `serde`, `serde_json`
- `mcp`
  - MCP SDK: `modelcontextprotocol` (Rust SDK)
  - Runtime: `tokio`
  - Optional: reuse `jsonrpsee` client to reach the daemon
- `apps/orchestra`
  - Minimal; depends on `cli` crate

All new dependencies should be added via `cargo add <pkg>` at latest versions; avoid editing `Cargo.toml` manually.

## Testing (concise)

- Fake agent binary for deterministic PTY behavior.
- Integration tests: temp git repo, exercise `new/start/attach/idle/complete/merge/gc`.
- E2E: launch daemon, attach PTY, resize handling, idle transitions, merge flow.
- Minimal mocking; boundaries at PTY and filesystem.

## Implications

- The workspace will start with a maximum of four crates: `core`, `cli`, `mcp`, and `apps/orchestra` (binary). This keeps boundaries clear while minimizing boilerplate.
- The CLI is a thin JSON-RPC client; all orchestration remains in `core`.
- Logging remains centralized with `tracing` to `./.orchestra/logs.jsonl`.
- Git operations use `git2`; PTY uses `portable-pty`.
- Future expansion (e.g., multiple attachments, alternative transports) can be added without breaking the MVP decisions.

## Alternatives Considered

- Raw UDS transport for JSON-RPC to reduce overhead. Rejected for simplicity and familiarity; `hyperlocal` is sufficient.
- Multiple binaries (separate `orchestra-daemon`, `orchestra-cli`, `orchestra-mcp`). Rejected to minimize distribution complexity and cognitive load.
- Different CLI UI stacks (`owo-colors`, `tabled`). Chosen `yansi` + `comfy-table` for ergonomics and aesthetics.
- `crossterm` for colors and terminal functionality, but decided it is an overkill for now.
