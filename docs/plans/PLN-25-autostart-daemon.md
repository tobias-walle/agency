# PLAN: Centralized autostart + version-aware restart for daemon
Ensure Agency always starts its daemon automatically for commands, and restarts it when the CLI version changes, using a single centralized helper.

## Goals
- Autostart daemon on all commands except `agency daemon ...`.
- Restart daemon when an already-running daemon reports a different version than the CLI.
- Centralize logic in a single helper to avoid per-command duplication.
- Add env opt-out for CI or special cases (`AGENCY_NO_AUTOSTART=1`).
- Keep TUI and setup behavior sensible (no autostart during setup guidance).

## Out of scope
- Persistent session storage or daemon state beyond current behavior.
- Changing socket location or version namespacing.
- PTY session lifecycle changes (tmux remains the session backend).
- Windows support.

## Current Behavior
- CLI entry and dispatch: `crates/agency/src/lib.rs` routes to subcommands; no daemon autostart occurs before commands. The default TUI path is entered when `None` command and global config exists.
- Daemon lifecycle commands: `crates/agency/src/commands/daemon.rs` provide `start`, `stop`, `restart`, `run_blocking` and handle socket creation and shutdown.
- Daemon process: `crates/agency/src/daemon.rs` runs a Unix socket listener and handles control frames such as `ListSessions`, `SubscribeEvents`, `NotifyTasksChanged`, `StopTask`, `Shutdown`, `Ping`.
- Client utilities: `crates/agency/src/utils/daemon.rs` offer `connect_daemon`, `connect_daemon_socket`, `send_message_to_daemon`, `list_sessions_for_project` and notifications; they bail when the daemon is not running.
- Socket path resolution: `crates/agency/src/config.rs::compute_socket_path` respects env, config, XDG or fallback to `~/.local/run/agency.sock` and ensures directory permissions.
- Commands expecting daemon: `crates/agency/src/commands/ps.rs`, `crates/agency/src/commands/sessions.rs`, `crates/agency/src/commands/attach.rs`, and `crates/agency/src/tui/mod.rs` rely on the daemon for session queries or UI.
- Tests: `crates/agency/tests/cli.rs` include cases asserting failure when the daemon is not running (e.g., `ps_bails_when_daemon_not_running`, `sessions_bails_when_daemon_not_running`).

## Solution
- Create `ensure_running_and_latest_version(ctx)` in `utils/daemon` to centralize autostart and version handling.
  - If connecting to the socket fails, start the daemon and skip the version check.
  - If connect succeeds, query daemon version via a new protocol message and compare to CLI `env!("CARGO_PKG_VERSION")`; on mismatch, restart the daemon.
  - Respect `AGENCY_NO_AUTOSTART=1` to disable autostart and version handling.
  - Use a short read timeout; older daemons without version support are treated as mismatch and restarted.
- Extend the daemon protocol with `GetVersion` and `Version { version }` messages.
- Implement `GetVersion` handler in the daemon that replies with `env!("CARGO_PKG_VERSION")`.
- Wire autostart centrally in `lib.rs` before dispatching commands, skipping for `Commands::Daemon { .. }`. In the `None` TUI path, only autostart after confirming the global config exists to avoid starting during setup.

## Architecture
- crates/agency/src/utils/daemon.rs
  - + `ensure_running_and_latest_version(ctx: &AppContext) -> anyhow::Result<()>`
  - Uses `compute_socket_path`, `connect_daemon_socket`, and `commands::daemon::{start, restart}`.
- crates/agency/src/daemon_protocol.rs
  - + `C2DControl::GetVersion`
  - + `D2CControl::Version { version: String }`
- crates/agency/src/daemon.rs
  - + Handler in `handle_connection` for `GetVersion` → reply `Version` with the daemon version.
- crates/agency/src/lib.rs
  - + Invoke `ensure_running_and_latest_version(&ctx)` early for all commands except `Daemon`.
  - In `None` branch, call autostart only after `global_config_exists()`.
- crates/agency/tests/cli.rs
  - ~ Update tests that expect “daemon not running” to set `AGENCY_NO_AUTOSTART=1` or replace with autostart assertions.
  - + Add new tests for autostart and version mismatch restart.

## Testing
- Unit
  - Protocol encode/decode for `GetVersion` and `Version` in `daemon_protocol.rs`.
  - Existing config path tests remain valid.
- Integration
  - Autostart: Running `ps` without daemon starts it and succeeds.
  - Opt-out: With `AGENCY_NO_AUTOSTART=1`, `ps` fails with the “Daemon not running...” message.
  - Version mismatch: When daemon reports a different version or unexpected reply, CLI restarts the daemon.
- E2E
  - `agency new` and attach flow continues to work; daemon is maintained automatically.

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
1. [ ] Add protocol variants in `crates/agency/src/daemon_protocol.rs`
   - Introduce `C2DControl::GetVersion` and `D2CControl::Version { version: String }`.
   - Validate compile and minimal behavior via unit checks.
2. [ ] Implement version handler in `crates/agency/src/daemon.rs`
   - In `handle_connection`, match on `GetVersion`; reply `Version { version: env!("CARGO_PKG_VERSION").to_string() }`.
3. [ ] Implement `ensure_running_and_latest_version(ctx)` in `crates/agency/src/utils/daemon.rs`
   - If `AGENCY_NO_AUTOSTART=1`, return early.
   - Compute socket; try `UnixStream::connect`.
   - On failure: call `commands::daemon::start()` and return Ok.
   - On success: send `GetVersion`; read with a short timeout; compare to CLI version; on mismatch or unexpected/timeout, call `commands::daemon::restart()`.
4. [ ] Wire centralized autostart in `crates/agency/src/lib.rs`
   - Before subcommand match, call `ensure_running_and_latest_version(&ctx)` unless `Commands::Daemon { .. }`.
   - In the `None` TUI path, only call after `global_config_exists()`.
5. [ ] Update tests in `crates/agency/tests/cli.rs`
   - Gate existing “daemon not running” tests with `AGENCY_NO_AUTOSTART=1`.
   - Add a test that `ps` autostarts and succeeds.
   - Add a test simulating version mismatch behavior to assert restart is triggered.
6. [ ] Run checks and formatting
   - Run `just check` and fix issues.
   - Run `just test` to verify.
   - Run `just fmt` or `just fix` if needed.
7. [ ] Commit changes following Conventional Commits
   - `feat: autostart daemon and restart on version mismatch for centralized reliability`

## Questions
1. Keep `AGENCY_NO_AUTOSTART=1` opt-out? Assumed: Yes, to keep CI stability and preserve tests that assert failures.
2. Autostart placement relative to setup and TUI? Assumed: Skip autostart when global config is missing; after setup, autostart applies before TUI run.
3. Read timeout for `GetVersion`? Assumed: ~250 ms; minimal delay and treats older daemons as mismatch to force upgrade.
4. Skip autostart for `Commands::Daemon { .. }`? Assumed: Yes, to avoid loops and unintended side effects.
5. Any need to differentiate minor vs patch version mismatch? Assumed: No; any mismatch restarts to keep daemon and CLI aligned.

