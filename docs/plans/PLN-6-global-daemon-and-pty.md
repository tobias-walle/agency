# PLN-6: Global daemon and unified PTY attach

Date: 2025-11-05

Introduce a single, global daemon socket and a unified attach client that orchestrate one PTY session at a time.
Extend the CLI with daemon subcommands and task-scoped attach/stop, wire `agency new` to start the daemon and attach.

## Goals

- Define `agency daemon start|stop|restart` as subcommands.
- Add `agency attach {task}` and `agency stop {task}` using `TaskRef` internally.
- Integrate PTY modules (protocol, session, daemon, client) lifted from `pty-demo`.
- Centralize attach logic in one place and reuse it across commands.
- Make daemon socket path configurable via Agency TOML with a secure default.
- Prefix all PTY tests with `pty_` and adapt them to Agency.
- Support only one PTY at a time through the global daemon.
- Clarify `TaskRef` semantics: it is a newtype representing either the task `id` or `slug`.

## Non Goals

- Multi-PTY routing/multiplexing over a single socket (defer to future plan).
- Windows support.
- Concurrent attachments to the same session.
- Full agent command integration beyond `$AGENCY_TASK` placeholder.

## Current Behavior

- Agency CLI commands exist: `new`, `path`, `branch`, `rm`, `ps` under `crates/agency/src/commands/*.rs`.
- Central dispatch in `crates/agency/src/lib.rs` with `clap` subcommands.
- Paths and config handled in `crates/agency/src/config.rs` with embedded defaults from `crates/agency/defaults/agency.toml`.
- Utilities in `crates/agency/src/utils/` for tasks (`task.rs`), git (`git.rs`), and terminal table/confirm (`term.rs`).
- No PTY/daemon exists in Agency currently.
- `pty-demo` (symlinked `./pty-demo`) contains working PTY modules and tests:
  - `src/protocol.rs` – framed message protocol and channels.
  - `src/session.rs` – PTY lifecycle, `vt100` screen, IO pumps.
  - `src/daemon.rs` – single-client attach lifecycle and restart-on-exit.
  - `src/client.rs` – attach client orchestrator.
  - `src/client/tty.rs` – raw mode helpers.
  - Tests under `pty-demo/tests/` for attach/detach, reject second attach, heavy output, restart.

## Solution

- Add a global daemon bound to one Unix socket path configurable via optional `[daemon] socket_path` override in Agency TOML.
  - Do not include `daemon.socket_path` in the default `defaults/agency.toml`; rely on a secure computed default when unset.
  - Sensitive default: prefer `XDG_RUNTIME_DIR/agency.sock`; fallback to a per-user path (e.g. `~/.local/run/agency.sock`) with parent directory permissions `0700`.
- Lift PTY modules from `pty-demo` to `crates/agency/src/pty/` and adapt imports and IO style.
- Centralize attach logic: `pty::client::run_attach(socket_path)` reused by `agency attach` and `agency new`.
- Extend CLI:
  - `daemon` subcommands: `start`, `stop`, `restart`.
  - `attach {task}` and `stop {task}` (CLI field name `task`, internal identity `TaskRef`).
- For `agency new <slug>`:
  - Create task/worktree, then start global daemon with placeholder agent command and env, allow `$AGENCY_TASK` to be provided via env, then attach.
- Keep single-client semantics and reject concurrent attaches.
- Port PTY tests to Agency, rename files to `pty_*`, and adapt helpers to Agency paths and CLI.

## Detailed Plan

1. [ ] Add dependencies via `cargo add` in the `agency` crate
   - [ ] Runtime: `portable-pty`, `crossterm`, `vt100`, `crossbeam-channel`, `serde` (derive), `bincode` (serde), `log`, `env_logger`.
   - [ ] Dev: `serial_test`, `expectrl`.
2. [ ] Extend configuration (`crates/agency/src/config.rs`)
   - [ ] Add `DaemonConfig { socket_path: Option<String> }` and include under `AgencyConfig { daemon: DaemonConfig }`.
   - [ ] Implement `fn default_socket_path() -> PathBuf`:
     - [ ] If `XDG_RUNTIME_DIR` is set, use `XDG_RUNTIME_DIR/agency.sock`.
     - [ ] Else fallback to per-user directory (e.g. `~/.local/run/`) and ensure parent directory exists with `0700` perms.
   - [ ] Provide `fn socket_path(cfg: &AgencyConfig) -> PathBuf` using config or default.
   - [ ] Do not add a `[daemon]` table to `crates/agency/defaults/agency.toml`; the default path is computed when not set.
3. [ ] Create PTY module structure under `crates/agency/src/pty/`
   - [ ] `mod.rs` facade exposing `protocol`, `session`, `daemon`, `client`, `paths`, `utils::tty`.
   - [ ] `protocol.rs`: copy from `pty-demo/src/protocol.rs` (adjust crate/module paths).
   - [ ] `session.rs`: copy from `pty-demo/src/session.rs`.
     - [ ] Replace the existing constructor with `Session::new(rows, cols, cmd, env)`.
     - [ ] Prefer borrowed parameters where possible (avoid unnecessary ownership per Rust best practices).
     - [ ] If `cmd` is empty, `bail!` instead of defaulting to `sh`.
   - [ ] `daemon.rs`: copy from `pty-demo/src/daemon.rs`.
     - [ ] Bind socket using `socket_path(&ctx.config)`.
     - [ ] Keep single session and single-client behavior.
     - [ ] Integrate `Session::new` using agent command with env-variable substitution (`$VAR`, `${VAR}`), including support for `$AGENCY_TASK` provided via the environment.
     - [ ] Ensure parent dir created with `0700` perms (`ensure_socket_dir_and_bind`).
   - [ ] `client.rs`: copy from `pty-demo/src/client.rs`.
     - [ ] Expose `pub fn run_attach(socket_path: &std::path::Path) -> anyhow::Result<()>`.
     - [ ] Use `anstream::eprintln` for user-facing errors.
   - [ ] `utils/tty.rs`: copy raw mode helpers (renamed from `client/tty.rs`), adjust imports.
   - [ ] `paths.rs`: helpers for `socket_path(&AgencyConfig)`, and optionally a `pid_path(&AgencyConfig)`.
4. [ ] CLI updates (`crates/agency/src/lib.rs`)
   - [ ] Update `Commands` enum:
     - [ ] Add `Daemon { #[command(subcommand)] action: DaemonAction }`.
     - [ ] Add `Attach { task: String }`.
     - [ ] Add `Stop { task: String }`.
   - [ ] Define `enum DaemonAction { Start, Stop, Restart }`.
   - [ ] Dispatch to new command modules.
5. [ ] Implement `commands/daemon.rs`
   - [ ] `start(ctx: &AppContext)`:
     - [ ] If PID file exists and process is alive, print "Daemon already running" and return.
     - [ ] Spawn daemon as detached child process bound to `socket_path`.
     - [ ] Initialize `env_logger`, write PID file, and print success.
     - [ ] Configure session to run agent placeholder command with env-variable substitution (including `$AGENCY_TASK`).
   - [ ] `stop(ctx: &AppContext)`:
     - [ ] Read PID, send SIGTERM; remove socket and PID files; print confirmation.
   - [ ] `restart(ctx: &AppContext)`:
     - [ ] `stop` then `start`.
6. [ ] Implement `commands/attach.rs`
   - [ ] Resolve `task: String` to `TaskRef` using `utils::task::resolve_id_or_slug` (internal identity).
   - [ ] Compute socket path via `socket_path(&ctx.config)`.
   - [ ] Call `pty::client::run_attach(&socket_path)`.
7. [ ] Implement `commands/stop.rs` (task-scoped convenience)
   - [ ] Resolve `task` to `TaskRef`.
   - [ ] For this single-session phase, delegate to `daemon::stop` (stops the daemon) and print note that one session is supported.
8. [ ] Integrate `agency new <slug>` (`crates/agency/src/commands/new.rs`)
   - [ ] After writing task file and creating branch/worktree:
     - [ ] Stop any running daemon.
     - [ ] Start daemon with agent placeholder command that supports env-variable substitution; allow providing `AGENCY_TASK` via env.
     - [ ] Attach using `pty::client::run_attach(&socket_path)`.
9. [ ] Tests (prefix with `pty_`, under `crates/agency/tests/`, `#[cfg(unix)]` and `#[serial]`)
   - [ ] `pty_helpers.rs`:
     - [ ] `bin()` returns Agency binary.
     - [ ] `spawn_daemon()` calls `agency daemon start`.
     - [ ] `wait_for_socket()` polls for the socket path from test-specific `.agency/agency.toml`.
     - [ ] `spawn_attach_pty(task)` runs `agency attach <task>`.
     - [ ] `send_ctrl_c()` sends `\x03`.
     - [ ] Write per-test `.agency/agency.toml` with:
       - [ ] `[daemon] socket_path = "./tmp/daemon.sock"` (relative to temp dir).
   - [ ] Use `expectrl` for terminal interaction in PTY tests.
   - [ ] Add `./scripts/fake_agent.py` that simulates a fast agent (no network, no timeouts), and use it as the placeholder command in tests to avoid flakiness and API costs.
   - [ ] `pty_attach.rs` (from `attach.rs`):
     - [ ] Start daemon, attach, `echo READY`, expect `READY`, Ctrl-C, EOF, stop daemon.
   - [ ] `pty_slow_client.rs` (from `slow_client.rs`):
     - [ ] Heavy output (`yes X | head -c 1000000`), Ctrl-C, EOF promptly.
   - [ ] `pty_shell_exit.rs` (from `shell_exit.rs`):
     - [ ] `exit` triggers stats and restart, remains responsive, Ctrl-C, EOF.
10. [ ] Logging & IO
    - [ ] Daemon: initialize `env_logger`, use `log` macros; avoid holding locks while sending frames.
    - [ ] CLI prints: use `anstream::println/eprintln`; avoid color assertions in tests.
11. [ ] Housekeeping
    - [ ] Run `just check` and address clippy.
    - [ ] Run `just test` and ensure reliability (prefer polling over sleeps).
    - [ ] Run `just fmt` to enforce formatting.

## Notes

- Attach logic is defined in a single place: `pty::client::run_attach`, reused by `agency attach` and `agency new`.
- We adopt a single global socket with a secure default (prefer `XDG_RUNTIME_DIR`).
  Tests override it per temp dir via a test-specific config.
- One PTY session at a time for this phase; `agency new` restarts the daemon and attaches to the fresh session.
- The agent placeholder command uses `agents.opencode.cmd` (or test fake agent) with env-variable substitution like in Dockerfiles (supports `$VAR` and `${VAR}`), and honors `$AGENCY_TASK` when provided.
- Define a `TaskRef` newtype to clearly represent either `id` or `slug`.
- Use `bail!` for CLI errors to ensure TTY-aware stderr with `anstream`.
