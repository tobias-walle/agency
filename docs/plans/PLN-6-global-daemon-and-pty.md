# PLN-6: Global daemon and unified PTY attach

Date: 2025-11-05

Introduce a single global daemon socket with a unified PTY attach client and extend the CLI with daemon subcommands plus task-scoped attach/stop; optionally attach from `agency new`.

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

- CLI offers `daemon` and `attach` as flat subcommands, without `start|stop|restart` or task arguments.
  - Command definitions live in `crates/agency/src/lib.rs:20` and route to `Daemon {}` and `Attach {}`.
  - `daemon` implementation calls `pty::daemon::run_daemon()` and blocks: `crates/agency/src/commands/daemon.rs:1`.
  - `attach` implementation calls `pty::client::run_attach()`: `crates/agency/src/commands/attach.rs:1`.
- PTY modules are already integrated under `crates/agency/src/pty/` (client, daemon, protocol, session):
  - Default socket path is hard-coded as `./tmp/daemon.sock`: `crates/agency/src/pty/config.rs:11`.
  - Daemon runs a single PTY session and rejects concurrent attaches: `crates/agency/src/pty/daemon.rs:29`.
  - Session currently launches a plain shell (`sh`) and restarts on exit: `crates/agency/src/pty/session.rs:27` and `crates/agency/src/pty/session.rs:63`.
- Config parsing is present, but has no `[daemon]` section; only `agents.*.cmd` is modeled:
  - Types and merge logic: `crates/agency/src/config.rs:1`.
  - Defaults: `crates/agency/defaults/agency.toml:1`.
- Task identity helpers exist and are used by other commands (`path`, `branch`, `rm`), but not by `attach`:
  - `TaskRef` and resolvers: `crates/agency/src/utils/task.rs:1`.
- Tests for PTY behavior exist and are prefixed with `pty_`:
  - `crates/agency/tests/pty_attach.rs:1`, `crates/agency/tests/pty_shell_exit.rs:1`, `crates/agency/tests/pty_slow_client.rs:1`.
  - Tests assume the default relative socket path `./tmp/daemon.sock`: `crates/agency/tests/pty_attach.rs:23`.

## Solution

- Move default socket to XDG: compute `XDG_RUNTIME_DIR/agency.sock`, fallback to per-user `~/.local/run/agency.sock` with parent permissions `0700`.
  - Make the socket path configurable via optional `[daemon] socket_path` in Agency TOML.
  - Remove reliance on the hard-coded relative default.
- Keep single global daemon and single-client semantics as-is.
- Centralize attach logic: expose `pty::client::run_attach(socket_path)` and reuse it from `agency attach` and, optionally, `agency new --attach`.
- Extend CLI:
  - `daemon` subcommands: `start`, `stop`, `restart`, plus a hidden `run` used as the long-lived daemon process.
  - `attach {task}` and `stop {task}` (CLI field `task`, internal identity `TaskRef`).
  - `new <slug> [--attach]`: only attach when `--attach` is supplied.
- Stop semantics: add protocol-level `Shutdown` and implement `agency daemon stop` by connecting to the socket and sending `Shutdown`.
- Agent command: add a new default agent `agents.fake.cmd = ["./scripts/fake_agent.py", "$AGENCY_TASK"]` and write `./scripts/fake_agent.py` to simulate a basic agent (no timeouts/costs).
- Refactor PTY to accept explicit socket path instead of a constant.
- Update PTY tests to pick up the socket path from `XDG_RUNTIME_DIR` (temp dir per test) or project config; keep names prefixed with `pty_`.

## Detailed Plan

1. [ ] Tests first: XDG socket and daemon CLI
   - Add `crates/agency/tests/pty_daemon_cli.rs` that sets `XDG_RUNTIME_DIR` to a temp dir and verifies:
     - `agency daemon start` returns quickly and creates `${XDG_RUNTIME_DIR}/agency.sock`.
     - `agency daemon stop` sends protocol `Shutdown` and the socket disappears.
     - `agency daemon restart` works and preserves single-client semantics.
   - Avoid sleeps; use polling with bounded timeouts as in existing helpers.
   - Use `temp-env` to set and restore `XDG_RUNTIME_DIR`.

2. [ ] Tests: attach/stop by task and new --attach
   - Add `crates/agency/tests/pty_attach_with_task.rs`:
     - Create a sample task file `.agency/tasks/1-alpha.md` with text.
     - Run `agency attach alpha` and verify attach handshake and output.
     - Run `agency stop alpha` and verify the daemon stops via `Shutdown`.
   - Add `crates/agency/tests/pty_new_attach_flag.rs`:
     - `agency new alpha` does not attach by default.
     - `agency new alpha --attach` ensures daemon is running and attaches.

3. [ ] Config: daemon section and socket computation
   - In `crates/agency/src/config.rs`, add `DaemonConfig { socket_path: Option<String> }` and `daemon: Option<DaemonConfig>` to `AgencyConfig`.
   - Implement `fn compute_socket_path(cfg: &AgencyConfig) -> PathBuf`:
     - Use `cfg.daemon.socket_path` if present.
     - Else compute `XDG_RUNTIME_DIR/agency.sock` or `~/.local/run/agency.sock` and `fs::create_dir_all` the parent with `0o700`.
   - Expose `compute_socket_path` via `AppContext` or a helper so commands can reuse it.

4. [ ] PTY interface refactor to take explicit paths
   - Change `pty::client::run_attach()` to `run_attach(socket_path: &Path)` and update `crates/agency/src/commands/attach.rs` to compute and pass it.
   - Change `pty::daemon::run_daemon()` to `run_daemon(socket_path: &Path)` and update `crates/agency/src/commands/daemon.rs` accordingly.
   - Remove `DEFAULT_SOCKET_PATH` from `crates/agency/src/pty/config.rs`.

5. [ ] CLI: daemon subcommands and wiring
   - In `crates/agency/src/lib.rs`, change `Commands::Daemon {}` to `Daemon { #[command(subcommand)] cmd: DaemonCmd }` and implement `DaemonCmd::{Start, Stop, Restart, Run}`.
   - `start`: spawn a detached child `agency daemon run` (blocking mode) and exit immediately after readiness.
   - `stop`: connect to the socket, send `Shutdown`, and wait for the socket to vanish.
   - `restart`: stop if running, then start.
   - Ensure CLI output uses `anstream::println` and errors use `bail!`.

6. [ ] CLI: attach/stop taking a task
   - Change `Attach` to `Attach { task: String }` and resolve with `utils::task::resolve_id_or_slug`.
   - Add `Stop { task: String }` (new command) and resolve similarly.
   - Store current task identity in daemon state to validate `stop <task>` targets the active session.
   - Update `crates/agency/src/commands/new.rs` to support `--attach`; when present, ensure the daemon is running (`start` if needed) and call attach with the new task.

7. [ ] Agent command integration with fake agent
   - Write `./scripts/fake_agent.py` that reads task text from `$AGENCY_TASK` and runs a basic REPL that echoes inputs with simple prefixes; ensure no network/timeouts are needed.
   - Add `agents.fake.cmd = ["./scripts/fake_agent.py", "$AGENCY_TASK"]` to `crates/agency/defaults/agency.toml`.
   - In `pty::session`, replace `sh` with the configured agent command (default: `agents.fake.cmd`), expanding `$AGENCY_TASK` from the resolved task.
   - Preserve restart-on-exit behavior; restart reuses the same task context and command.

8. [ ] Update existing PTY tests
   - Replace assumptions about `./tmp/daemon.sock` with `XDG_RUNTIME_DIR` pointing to `workdir.join("tmp")` (via `temp-env`).
   - Keep existing PTY behavior checks (single-client, heavy output, shell exit semantics), adapted for the fake agent.
   - Ensure serial execution where needed using `serial_test`.

9. [ ] Docs and help
   - Update CLI help to document `daemon start|stop|restart|run`, `attach {task}`, `stop {task}`, and `new --attach`.
   - Document socket path resolution (XDG default, fallback path/permissions) in `README.md`.
   - Add brief usage examples referencing `just` helpers.

10. [ ] Lint and format
   - Run `just check` and fix all warnings/errors.
   - Run `cargo fmt`.

## Notes

- We switch to XDG socket default now; tests will set `XDG_RUNTIME_DIR` to a temp directory for isolation.
- Stop uses a protocol-level `Shutdown`; we do not rely on PID files or signals.
- The `fake_agent.py` keeps CI deterministic and fast; later we can add real agents without changing daemon/attach semantics.
 - No async runtimes; use threads only. Prefer `println!`/`eprintln!` from `anstream` and `bail!` for error cases.
