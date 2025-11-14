# PLAN: Move to tmux-only sessions with slim daemon
Replace PTY stack with tmux, keep a slim daemon for status/notifications. Attach directly into tmux UI.

## Goals
- Remove all `pty/*` code and custom PTY multiplexing
- Use a dedicated tmux server/socket with its own env and config keys
- Keep a slim daemon to compute statuses (Running/Idle/Exited/Stopped/Completed) and notify clients
- Attach directly to tmux TUI; no custom rendering or snapshots
- Preserve Completed status and existing CLI/TUI UX

## Out of scope
- Windows support or PTY fallback
- Streaming PTY data to clients
- Deep tmux configuration beyond isolation and remain-on-exit

## Current Behavior
- PTY/daemon implementation and dependencies
  - `crates/agency/src/pty/daemon.rs:1` UDS server streaming frames (`D2C`, `C2D`), managing PTY sessions and clients
  - `crates/agency/src/pty/session.rs:1` `portable_pty` child, vt100 parsing, resize/restart/sinks
  - `crates/agency/src/pty/client.rs:1` raw-mode client with input/resize/output threads, detach/restart
  - `crates/agency/src/pty/protocol.rs:1` frame protocol, channels, `SessionInfo`, `Welcome`, `Exited`, `Output`
  - `crates/agency/src/pty/registry.rs:1` session registry with idle/exited polling and broadcasts
- CLI/TUI usage of daemon
  - `crates/agency/src/commands/daemon.rs:1` start/stop/run daemon
  - `crates/agency/src/commands/start.rs:1` prepares worktree, then `pty_client::run_attach(...)`
  - `crates/agency/src/commands/attach.rs:1` joins/opens via `pty_client::run_attach(...)`
  - `crates/agency/src/commands/sessions.rs:1`, `crates/agency/src/commands/ps.rs:1` list via daemon
  - `crates/agency/src/utils/daemon.rs:1` UDS helpers (connect, list, notify)
  - `crates/agency/src/tui/mod.rs:1` subscribes to daemon events and attaches via PTY client
- Completed status
  - `crates/agency/src/utils/status.rs:1` uses `.agency/state/completed/<id>-<slug>`; `ps` and TUI override to Completed when marker exists

## Solution
- Remove PTY stack and frame streaming; rely on tmux-only sessions
- Introduce tmux integration
  - Isolated server: `tmux -S <tmux_socket> -L agency`; always unset `TMUX` for Agency subprocesses
  - Session naming: `agency-<task-id>-<slug>`; derive mapping from names; use `#{session_id}` for runtime handle
  - Start with `remain-on-exit on` and enable `pipe-pane -o` per session to update an activity stamp for Idle detection
- Keep a slim daemon for status/notifications
  - Bind UDS at existing `compute_socket_path` (unchanged)
  - Maintain in-memory registry populated from tmux (`list-sessions`, `list-panes`) and activity stamps; compute statuses:
    - Running: session exists
    - Idle: no output for ≥1s (stamp mtime)
    - Exited: pane dead via `#{pane_dead}` with `remain-on-exit on`
    - Stopped/Draft: derived from worktree presence when no session
    - Completed: override when marker exists
  - Broadcast `SessionsChanged`/`TasksChanged` to subscribers via UDS
- CLI
  - `start`: daemon prepares worktree/branch, starts tmux session (detached), enables pipe; CLI attaches directly to tmux
  - `attach`: if missing, ask daemon to start; then attach directly to tmux
  - `stop`: daemon kills session(s), disables pipe, removes stamps, broadcasts
  - `sessions`/`ps`: query daemon for computed `SessionInfo`
- TUI
  - Keep daemon event subscription; attach via tmux on Enter using interactive scope
- Config keys
  - Dedicated tmux socket: env `AGENCY_TMUX_SOCKET_PATH` and config `daemon.tmux_socket_path`
  - Keep daemon UDS keys: env `AGENCY_SOCKET_PATH` and config `daemon.socket_path`

## Architecture
- Deleted
  - `crates/agency/src/pty/client.rs`
  - `crates/agency/src/pty/daemon.rs`
  - `crates/agency/src/pty/idle.rs`
  - `crates/agency/src/pty/mod.rs`
  - `crates/agency/src/pty/protocol.rs`
  - `crates/agency/src/pty/registry.rs`
  - `crates/agency/src/pty/session.rs`
  - `crates/agency/src/pty/transcript.rs`
  - `crates/agency/src/utils/daemon.rs` (old helpers)
- Added
  - `crates/agency/src/utils/tmux.rs`
    - `tmux_socket_path(cfg)` from `AGENCY_TMUX_SOCKET_PATH` → `cfg.daemon.tmux_socket_path` → repo-local default
    - `tmux_args_base(cfg) -> Vec<String>`: `-S`, `-L agency`, unset `TMUX`
    - `session_name(task_id, slug)`
    - `start_session(task, worktree, argv, env)`; set `remain-on-exit on`; enable `pipe-pane -o` to write activity stamp
    - `attach_session(task)`; `exec tmux attach`
    - `kill_session(task)`; best-effort disable pipe
    - `list_sessions(project_root) -> Vec<TmuxRow>`; parse `#{session_name}`, `#{session_id}`, `#{session_created}`
    - `pane_dead(name) -> bool` via `list-panes -F '#{pane_dead}' -t <name>`
  - `crates/agency/src/daemon_protocol.rs`
    - `ProjectKey`, `TaskMeta`, `SessionInfo` (fields used by `ps`/TUI)
    - `C2DControl`: `StartTask`, `StopTask`, `ListSessions`, `SubscribeEvents`, `NotifyTasksChanged`, `Shutdown`, `Ping`
    - `D2CControl`: `Sessions`, `SessionsChanged`, `TasksChanged`, `Ack`, `Error`, `Pong`
    - `write_frame`, `read_frame` (control-only)
  - `crates/agency/src/daemon.rs` (slim daemon)
    - UDS bind at `compute_socket_path`
    - Poll loop (~250ms): list tmux, check `pane_dead`, read activity stamps; compute status; diff → broadcast `SessionsChanged`
    - Control handlers: `StartTask`, `StopTask`, `ListSessions`, `SubscribeEvents`, `NotifyTasksChanged`, `Shutdown`
    - Cleanup: disable pipe, remove stamps; prune orphan stamps (TTL 24h)
- Modified
  - `crates/agency/src/commands/start.rs:1`: RPC `StartTask` then `interactive::scope(|| tmux::attach_session(...))`
  - `crates/agency/src/commands/attach.rs:1`: resolve task; if missing, RPC `StartTask`; attach via tmux
  - `crates/agency/src/commands/stop.rs:1`: RPC `StopTask`; log Ack
  - `crates/agency/src/commands/sessions.rs:1`: query daemon `ListSessions`; render
  - `crates/agency/src/commands/ps.rs:1`: import `daemon_protocol::SessionInfo`; list via daemon
  - `crates/agency/src/utils/sessions.rs:1`: switch to `crate::daemon_protocol::SessionInfo`
  - `crates/agency/src/commands/daemon.rs:1`: start/stop/restart/run now call `daemon::run_daemon`
  - `crates/agency/src/tui/mod.rs:1`: swap protocol imports to `daemon_protocol`; keep event subscription; attach via tmux
  - `crates/agency/src/config.rs:240`: add `daemon.tmux_socket_path` and env `AGENCY_TMUX_SOCKET_PATH`; keep UDS logic for `compute_socket_path`
  - `README.md:70`: update architecture (tmux-only sessions + slim daemon) and document sockets

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
1. [ ] Add `utils/tmux.rs`
   - Build base args (`-S`, `-L agency`) and unset `TMUX`; implement `session_name`, `start/attach/kill/list`, `pane_dead`
   - Implement `enable_pipe(task, stamp)` writing mtime (e.g., `touch <stamp>` via pipe)
2. [ ] Add `daemon_protocol.rs`
   - Define control-only protocol (no `Output`), `SessionInfo`, framing helpers
3. [ ] Implement slim `daemon.rs`
   - Bind UDS at `compute_socket_path`
   - Registry: last-seen sessions + stamps; compute statuses (Idle threshold 1s; Exited via `pane_dead`)
   - Poll loop at ~250ms; diff and broadcast `SessionsChanged`; handle `NotifyTasksChanged`
   - Control handlers: `StartTask` (ensure branch/worktree, start tmux, enable pipe), `StopTask` (kill, cleanup), `ListSessions`, `SubscribeEvents`, `Shutdown`
4. [ ] Remove PTY stack
   - Delete `crates/agency/src/pty/*` and old `utils/daemon.rs`; remove `pub mod pty;` and fix imports
5. [ ] Wire CLI to new daemon
   - `commands/daemon.rs` → `daemon::run_daemon`
   - `start/attach/stop/sessions/ps` → RPC to daemon; attach via tmux directly for interactive paths
6. [ ] Update TUI
   - Replace protocol imports with `daemon_protocol`; retain subscriber path; on Enter, run `tmux attach` in `interactive::scope`
7. [ ] Config changes
   - Extend `AgencyConfig` with `daemon.tmux_socket_path`; implement `tmux_socket_path(cfg)` and env `AGENCY_TMUX_SOCKET_PATH`
   - Document both sockets: UDS vs tmux
8. [ ] Cleanup behavior
   - Ensure daemon disables pipe and removes stamps on `StopTask`; prune orphan stamps on startup and periodically (TTL 24h)
9. [ ] Preserve Completed
   - Continue using `is_task_completed(...)` to override status to “Completed” in `ps` and TUI
10. [ ] Docs and justfile
    - Update `README.md` architecture and detach behavior; add tmux socket docs
    - Keep `AGENCY_SOCKET_PATH` in `justfile` for UDS; add `AGENCY_TMUX_SOCKET_PATH` guidance
11. [ ] Tests and formatting
    - Remove PTY-dependent tests and any assumptions about framed PTY bytes
      - Delete tests relying on `crates/agency/src/pty/*` types or handshake frames
      - Remove assertions tied to old `utils/daemon.rs` hardcoded unreachable message if behavior changes
    - Adjust existing tests
      - Update `crates/agency/src/commands/ps.rs` unit tests to import `crate::daemon_protocol::SessionInfo`
      - Update `crates/agency/src/tui/mod.rs` row-building tests to use new `SessionInfo` and include Exited mapping
      - Keep Completed marker behavior intact (`is_task_completed`) in both `ps` and TUI
    - Add unit tests
      - `daemon_protocol.rs`: encode/decode `Sessions`, `SessionsChanged`, `Ack`, `Error`; negative decode path
      - `utils/status.rs`: derive `Running/Idle/Exited/Stopped/Draft` from `SessionInfo` + worktree presence; Completed override
      - `daemon.rs` helpers: idle detection via activity stamp mtime with 1s threshold; diff-and-broadcast emits `SessionsChanged` once
      - `daemon.rs` helpers: Exited detection maps mocked `pane_dead=true` to `Exited`
      - `utils/tmux.rs`: `session_name(id, slug)` → `agency-<id>-<slug>`; parse `list-sessions` rows into `(id, slug, session_id, created)`
      - `config.rs`: tmux socket precedence (`AGENCY_TMUX_SOCKET_PATH` > `daemon.tmux_socket_path` > default) and `0700` dirs
      - `tui/mod.rs`: subscriber path reacts to mocked `SessionsChanged`/`TasksChanged` (stub channel) and refreshes table
      - `daemon.rs` helpers: orphan/TTL pruning removes stale activity stamps
    - Optional guarded integration tests (skip when `tmux` unavailable)
      - `agency new --draft` → `agency start <id>` → `agency stop --task <id>` roundtrip
      - `sessions` shows correct session naming and mapping
    - Run `just check` and fix warnings; run `just fmt`

## Questions
1. Session naming format `agency-<id>-<slug>` acceptable? Assumed Yes
2. Attach behavior when no session exists: auto-start via daemon or bail? Assumed auto-start
3. Idle threshold fixed to 1s; poll interval ~250ms acceptable? Assumed Yes
4. Config keys separation: UDS `daemon.socket_path`/`AGENCY_SOCKET_PATH` and tmux `daemon.tmux_socket_path`/`AGENCY_TMUX_SOCKET_PATH`? Assumed Yes
5. Exited handling via `remain-on-exit on` + `#{pane_dead}` sufficient? Assumed Yes
6. Restart flow: expose `agency start` to respawn when Exited; add `agency restart` later if needed? Assumed Yes
