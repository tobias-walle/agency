# PLAN: Align TUI keys with CLI and keymap updates
Ensure TUI keys map to CLI commands, add missing commands, add merge/open shortcuts, and change quit behavior to Ctrl-C only. Update Start behavior to start sessions in the background (no attach) and add a matching CLI command.

## Goals
- Map actionable TUI keys to dedicated `commands::*` entrypoints
- Add missing CLI commands: `edit` (open markdown) and `reset` (prune worktree/branch)
- Add TUI shortcuts: `m` for merge, `o` for open worktree
- Remove daemon auto-start from TUI actions; Start does not start the daemon if it is down
- Change quit to only Ctrl-C; Esc cancels slug modal only

## Out of scope
- Adding shortcuts beyond `m` and `o`
- PTY detach keybinding changes (handled by config and PTY client)
- Broader TUI layout/UX redesign

## Current Behavior
- TUI key handling is implemented in `crates/agency/src/tui/mod.rs` inside `ui_loop` under `match state.mode` (see around list handling).
  - Quit on `q` or `Esc` in list view (global): `KeyCode::Char('q') | KeyCode::Esc => break`
  - Selection: Up/Down and `j`/`k`
  - Enter:
    - If a session exists for the selected row: attaches via `commands::attach::run_join_session`
    - Otherwise: opens the task markdown directly via `utils::editor::open_path` (no CLI command abstracts this)
  - New task: `n` (create and open editor, no attach) and `N` (create, start daemon, attach)
  - Attach: `s` (starts daemon, then `attach`)
  - Stop sessions: `S` (logs a command, uses best-effort utils; does not invoke `commands::stop`)
  - Delete: `X` (non-interactive `commands::rm::run_force`)
  - Reset: `R` (logs a command, then inline utils: stop sessions, prune worktree, delete branch). No CLI `reset` command exists.
- CLI commands available in `crates/agency/src/lib.rs`: `new`, `merge`, `open`, `path`, `branch`, `rm`, `ps`, `daemon` (start/stop/restart/run), `attach`, `bootstrap`, `stop`, `sessions`, `tui`.
- PTY detach keybinding is configured in `utils::keybindings` and used in `pty/client`; unrelated to TUI key handling.

Identified gaps for parity:
- No CLI command to open the task markdown file (TUI Enter when no session)
- No CLI `reset` command to prune worktree and branch while keeping the file
- TUI starts the daemon for `s` and `N`, which is undesired
- No TUI shortcuts for `merge` and `open`

## Solution
- Add CLI commands:
  - `edit <ident>`: open the task markdown file in `$EDITOR` (mirrors Enter w/o session)
  - `reset <ident>`: best-effort stop sessions, prune worktree, delete branch, notify daemon; keep markdown file
- TUI keymap changes:
  - Quit only on Ctrl-C (Ctrl + c). Remove `q` and global `Esc` quit. Esc continues to cancel slug modal.
  - Enter (no session): call `commands::edit::run` rather than `utils::editor` directly.
  - Start selected task: `s` runs new `commands::start::run(&ctx, &id_str)` to start the session in the background (no attach). Do not auto-start the daemon; if down, the action can fail and should be logged in the Command Log.
  - New + Start: `N` creates the task and starts it in the background (no attach) via the same `start` command; no daemon auto-start.
  - `S` (Stop): use `commands::stop::run(&ctx, Some(id_str), None)`; allow failure if daemon is down and surface the error in the Command Log.
  - `R`: delegate to new `commands::reset::run` in a background thread; log `agency reset <id>`.
  - `m`: background `commands::merge::run(&ctx, &id_str, None)`; log `agency merge <id>`.
  - `o`: open worktree via `commands::open::run`; temporarily leave TUI (restore/reinit) like Enter.
  - Update help bar to reflect: `Start: s`, `Merge: m`, `Open: o`, and `Quit: ^C`.

## Architecture
- CLI
  - Update `crates/agency/src/lib.rs`
    - Add subcommands: `Edit { ident: String }`, `Reset { ident: String }`
    - Dispatch to `commands::edit::run` and `commands::reset::run`
    - Add subcommand: `Start { ident: String }` and dispatch to `commands::start::run`
  - Add `crates/agency/src/commands/edit.rs`
    - `pub fn run(ctx: &AppContext, ident: &str) -> Result<()>`
    - Resolve task (`utils::task::resolve_id_or_slug`), compute markdown (`task_file`), log, `utils::editor::open_path`
  - Add `crates/agency/src/commands/reset.rs`
    - `pub fn run(ctx: &AppContext, ident: &str) -> Result<()>`
    - Resolve task; best-effort `utils::daemon::stop_sessions_of_task`
    - Prune worktree, delete branch; `log_success!` confirmations; notify daemon
  - Add `crates/agency/src/commands/start.rs`
    - `pub fn run(ctx: &AppContext, ident: &str) -> Result<()>`
    - Resolve task and prepare worktree/branch (like `attach`), build `SessionOpenMeta`, then open a short-lived connection to the daemon socket and:
      - Send `C2D::Control(C2DControl::OpenSession { meta, rows: 24, cols: 80 })`
      - Read a single `D2C::Control(D2CControl::Welcome { .. })`
      - Send `C2D::Control(C2DControl::Detach)` and close the connection
    - Log success via `log_success!` and let errors bubble up (so TUI can log them)
- TUI
  - Update `crates/agency/src/tui/mod.rs`
    - Import `KeyModifiers`; change quit to Ctrl-C detection
    - Enter no-session → `commands::edit::run`
    - `s` → background `commands::start::run`; no daemon auto-start
    - `N` → create task then `commands::start::run`; no daemon auto-start
    - `S` → use `commands::stop::run`; on error, push error to Command Log
    - `R` → background `commands::reset::run`; keep command log
    - Add `m` and `o` handlers; log and act (with restore/reinit for `o`)
    - Update help bar string

## Detailed Plan
- [ ] Add `commands/edit.rs`
  - [ ] Resolve task and open markdown via `open_path`
  - [ ] Log using `log_info!` with `utils::log::t::path`
- [ ] Add `commands/reset.rs`
  - [ ] Resolve task; compute branch/worktree
  - [ ] Best-effort stop sessions; prune worktree; delete branch; notify
  - [ ] Log using `log_success!` and concise `log_warn!` where helpful
- [ ] Wire CLI in `crates/agency/src/lib.rs`
  - [ ] Add subcommands `Edit` and `Reset`
  - [ ] Route to `commands::edit::run` and `commands::reset::run`
- [ ] Update TUI in `crates/agency/src/tui/mod.rs`
  - [ ] Change quit to Ctrl-C only; limit Esc to modal cancel
  - [ ] Enter (no session) → `commands::edit::run`
  - [ ] `s` Start: call `commands::start::run` in background; no daemon auto-start
  - [ ] `N` New + Start: create then call `commands::start::run`; no daemon auto-start
  - [ ] `S` Stop: switch to `commands::stop::run` and surface failures in Command Log
  - [ ] `R` → background `commands::reset::run`
  - [ ] Add `m` (merge) and `o` (open)
  - [ ] Update help bar text: add `m`, `o`, change quit to `^C`
- [ ] Tests in `crates/agency/tests/cli.rs`
  - [ ] `edit` works without launching real editor (set a dummy `EDITOR`); assert success
  - [ ] `reset` removes worktree/branch and keeps markdown; assert idempotency and notifications
- [ ] Run `just check` and fix issues; then `just fmt`

## Questions
1) Help label for `s`: Keep "Start: s" even though it doesn’t attach. Assumed: Yes, "Start: s".
2) `S` should be allowed to fail if daemon is down and the error should be visible in the Command Log. Assumed: Switch to `commands::stop::run` and surface errors from the TUI thread.
3) For `o` (Open), confirm we temporarily leave TUI (restore/reinit) like Enter. Assumed: Yes, mirror Enter behavior.

Resolved decisions from user:
- TUI must not start the daemon (applies to `s` and `N`)
- Add shortcuts `m` (merge) and `o` (open); `o` leaves the TUI
- Quit only via Ctrl-C; Esc only cancels slug modal
- `s` starts the task in the background (no attach) and there will be a matching `start` command
- `S` can fail if daemon is down; errors should be logged in the Command Log
