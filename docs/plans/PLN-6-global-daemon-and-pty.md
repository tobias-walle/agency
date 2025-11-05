# Feedback
- Rename `effective_socket_path()` to `socket_path()`. I don't like the word effective
- Specify that `TaskRef` is either the `id` or the `slug` of the task
- Don't include `daemon.socket_path` in the default config toml
- Don't include `daemon.socket_path` in the default config toml
- `Session::new_with_cmd(rows, cols, cmd: Vec<String>, env: Vec<(String, String)>)` should be `Session::new(rows, cols, cmd: Vec<String>, env: Vec<(String, String)>)` (replacing the existing implementation). If possible non owned values should be passed following rust best practices.
- If `cmd` is empty `bail`  
- Can $AGENCY_TASK be provided as env var and all env vars be replaced in the command? It should work like in Dockerfiles.
- Renmae client/tty.rs to utils/tty.rs
- Define a struct (New Type) for the `TaskRef` String (to increase clarity
- Make sure to use `expectrl` for the tests
- For the tests create `./scripts/fake_agent.py` which simulates an agent like claude code or opencode, but is much faster and don't produces any api costs. Avoid timeouts in the fake agent to keep the tests fast.

# PLN-6: Global daemon and unified PTY attach

Date: 2025-11-05

Introduce a single, global daemon socket and a unified attach client that orchestrate one PTY session at a time. Extend the CLI with daemon subcommands and task-scoped attach/stop, wire `agency new` to start the daemon and attach.

## Goals

- Define `agency daemon start|stop|restart` as subcommands.
- Add `agency attach {task}` and `agency stop {task}` using `TaskRef` internally.
- Integrate PTY modules (protocol, session, daemon, client) lifted from `pty-demo`.
- Centralize attach logic in one place and reuse it across commands.
- Make daemon socket path configurable via Agency TOML with a secure default.
- Prefix all PTY tests with `pty_` and adapt them to Agency.
- Support only one PTY at a time through the global daemon.

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

- Add a global daemon bound to one Unix socket path configurable via `[daemon] socket_path` in Agency TOML.
  - Sensitive default: prefer `XDG_RUNTIME_DIR/agency.sock`; fallback to a per-user path (e.g. `~/.local/run/agency.sock`) with parent directory permissions `0700`.
- Lift PTY modules from `pty-demo` to `crates/agency/src/pty/` and adapt imports and IO style.
- Centralize attach logic: `pty::client::run_attach(socket_path)` reused by `agency attach` and `agency new`.
- Extend CLI:
  - `daemon` subcommands: `start`, `stop`, `restart`.
  - `attach {task}` and `stop {task}` (CLI field name `task`, internal identity `TaskRef`).
- For `agency new <slug>`:
  - Create task/worktree, then start global daemon with placeholder agent command and env `$AGENCY_TASK="Say Hello"` (resolved from config), then attach.
- Keep single-client semantics and reject concurrent attaches.
- Port PTY tests to Agency, rename files to `pty_*`, and adapt helpers to Agency paths and CLI.

## Detailed Plan

1. [ ] Add dependencies via `cargo add` in the `agency` crate
   - [ ] Runtime: `portable-pty`, `crossterm`, `vt100`, `crossbeam-channel`, `serde` (derive), `bincode` (serde), `log`, `env_logger`.
   - [ ] Dev: `serial_test`.
2. [ ] Extend configuration (`crates/agency/src/config.rs`, `crates/agency/defaults/agency.toml`)
   - [ ] Add `DaemonConfig { socket_path: Option<String> }` and include under `AgencyConfig { daemon: DaemonConfig }`.
   - [ ] Implement `fn default_socket_path() -> PathBuf`:
     - [ ] If `XDG_RUNTIME_DIR` is set, use `XDG_RUNTIME_DIR/agency.sock`.
     - [ ] Else fallback to per-user directory (e.g. `~/.local/run/`) and ensure parent directory exists with `0700` perms.
   - [ ] Provide `fn effective_socket_path(cfg: &AgencyConfig) -> PathBuf` using config or default.
   - [ ] Update `defaults/agency.toml` with:
     - [ ] `[[agents.opencode]]` unchanged.
     - [ ] Add `[daemon]` table with `socket_path = ""` (empty => use default).
3. [ ] Create PTY module structure under `crates/agency/src/pty/`
   - [ ] `mod.rs` facade exposing `protocol`, `session`, `daemon`, `client`, `paths`.
   - [ ] `protocol.rs`: copy from `pty-demo/src/protocol.rs` (adjust crate/module paths).
   - [ ] `session.rs`: copy from `pty-demo/src/session.rs`.
     - [ ] Add `Session::new_with_cmd(rows, cols, cmd: Vec<String>, env: Vec<(String, String)>)` to support agent placeholder.
     - [ ] Default to `sh` when `cmd` empty.
   - [ ] `daemon.rs`: copy from `pty-demo/src/daemon.rs`.
     - [ ] Bind socket using `effective_socket_path(&ctx.config)`.
     - [ ] Keep single session and single-client behavior.
     - [ ] Integrate `Session::new_with_cmd` using agent command from config (with `$AGENCY_TASK` substitution) and env.
     - [ ] Ensure parent dir created with `0700` perms (`ensure_socket_dir_and_bind`).
   - [ ] `client.rs`: copy from `pty-demo/src/client.rs`.
     - [ ] Expose `pub fn run_attach(socket_path: &std::path::Path) -> anyhow::Result<()>`.
     - [ ] Use `anstream::eprintln` for user-facing errors.
   - [ ] `client/tty.rs`: copy raw mode helpers, adjust imports.
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
     - [ ] Spawn daemon as detached child process bound to `effective_socket_path`.
     - [ ] Initialize `env_logger`, write PID file, and print success.
     - [ ] Configure session to run agent placeholder command from config with `$AGENCY_TASK`.
   - [ ] `stop(ctx: &AppContext)`:
     - [ ] Read PID, send SIGTERM; remove socket and PID files; print confirmation.
   - [ ] `restart(ctx: &AppContext)`:
     - [ ] `stop` then `start`.
6. [ ] Implement `commands/attach.rs`
   - [ ] Resolve `task: String` to `TaskRef` using `utils::task::resolve_id_or_slug` (internal identity).
   - [ ] Compute socket path via `effective_socket_path(&ctx.config)`.
   - [ ] Call `pty::client::run_attach(&socket_path)`.
7. [ ] Implement `commands/stop.rs` (task-scoped convenience)
   - [ ] Resolve `task` to `TaskRef`.
   - [ ] For this single-session phase, delegate to `daemon::stop` (stops the daemon) and print note that one session is supported.
8. [ ] Integrate `agency new <slug>` (`crates/agency/src/commands/new.rs`)
   - [ ] After writing task file and creating branch/worktree:
     - [ ] Stop any running daemon.
     - [ ] Start daemon with agent placeholder env `AGENCY_TASK="Say Hello"` and command from config (substitute `$AGENCY_TASK`).
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
- We adopt a single global socket with a secure default (prefer `XDG_RUNTIME_DIR`). Tests override it per temp dir.
- One PTY session at a time for this phase; `agency new` restarts the daemon and attaches to the fresh session.
- The agent placeholder command uses `agents.opencode.cmd` from config with `$AGENCY_TASK` substitution. If not set, default to running `sh` in the PTY.
- For multi-PTY support later, extend the protocol with task lifecycle messages, maintain a task registry in the daemon, and route frames per task over the single socket.
- Use `bail!` for CLI errors to ensure TTY-aware stderr with `anstream`.
