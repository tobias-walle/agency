# PLAN: Event-Driven TUI With Ratatui
Interactive minimal table TUI with bordered cells, highlighted selection + help bar, and daemon-broadcast updates. Launch by default (`agency`) and via `agency tui`. Non-TTY prints help. After detach, re-enter the TUI.

## Goals
- Render compact bordered table with selection highlight using ratatui
- Keybindings: Up/Down/j/k navigate; Enter edit/attach; S start; X delete task; R reset; n new draft; N new+start; q quit
- Bottom help bar highlighted; uppercase keys for shifted actions
- Event model: daemon broadcasts `TasksChanged { project }` and `SessionsChanged { entries }`; TUI subscribes and refreshes
- Default invocation launches TUI when no subcommand; if not TTY, print help and exit
- On detach from a session, return to the TUI seamlessly

## Out of scope
- Multi-pane UI, search/filtering, mouse input
- Daemon-side filesystem watching (clients notify via `NotifyTasksChanged`)
- Windows support and async runtimes

## Current Behavior
- CLI entry and dispatch in `crates/agency/src/lib.rs:56` (no default TUI when no subcommand)
- Task listing and latest session status in `crates/agency/src/commands/ps.rs:1`
- Attach/Join via daemon in `crates/agency/src/commands/attach.rs:1`
- Stop sessions via daemon in `crates/agency/src/commands/stop.rs:1`
- New task creation (file/worktree/branch) in `crates/agency/src/commands/new.rs:1`
- Remove task (file, branch, worktree) in `crates/agency/src/commands/rm.rs:1`
- Task utilities (id/slug resolution, paths, markdown) in `crates/agency/src/utils/task.rs:1`
- Editor helper in `crates/agency/src/utils/editor.rs:1`
- Daemon accept/registry/broadcast scaffolding in:
  - `crates/agency/src/pty/daemon.rs:1`
  - `crates/agency/src/pty/registry.rs:1`
  - Protocol types in `crates/agency/src/pty/protocol.rs:1`

## Solution
- Rendering
  - Use ratatui Table with headers [ID, SLUG, STATUS, SESSION]; per-cell borders (`Block::bordered()`)
  - Highlight selected row via `.highlight_style(Style::new().bg(Color::Gray))`
  - Bottom help bar with highlighted background and uppercase keys for shifted actions
  - Centered slug input overlay for `n`/`N`
- Events & protocol
  - Add `C2DControl::SubscribeEvents { project: ProjectKey }`
  - Add `C2DControl::NotifyTasksChanged { project: ProjectKey }`
  - Add `D2CControl::TasksChanged { project: ProjectKey }`
  - Add `D2CControl::SessionsChanged { entries: Vec<SessionInfo> }`
  - Clients subscribe; daemon maintains subscribers per project and broadcasts on open/join/stop/exit and notify
- Actions
  - Enter: if a session exists -> join; else open task in editor
  - S: start + attach selected task (ensure daemon)
  - X: delete everything (task file, branch, worktree, sessions) using `rm`/stop; then notify tasks changed
  - R: reset (stop sessions, delete branch/worktree only; keep task file); then notify tasks changed
  - n: create task, open editor, do not start; notify tasks changed
  - N: create task, open editor (block until editor closes), then start + attach; after detach, return to TUI; notify tasks changed
  - q: exit; if not TTY, print help and exit immediately
- Default run: launch TUI when `None` and TTY; otherwise print help

## Architecture
- Files
  - `crates/agency/src/tui/mod.rs`
    - `pub fn run(ctx: &AppContext) -> Result<()>`
    - `AppState`, `Mode { List, InputSlug }`, `TaskRow`
    - Rendering (table + help bar + input overlay)
    - Subscribe reader thread -> internal channel -> UI loop
  - `crates/agency/src/lib.rs`
    - Add `Tui {}` subcommand; default to TUI on `None` (TTY) or print help
  - `crates/agency/src/pty/protocol.rs`
    - Extend `C2DControl`/`D2CControl` with subscribe/notify/events
  - `crates/agency/src/pty/daemon.rs`
    - Subscribers list per `ProjectKey`; broadcast `TasksChanged`/`SessionsChanged`
  - `crates/agency/src/pty/registry.rs`
    - Helper to list sessions filtered by project
- Dependencies
  - Add `ratatui` and `crossterm` via `cargo add`

## Detailed Plan
- [x] Ratatui API references
  - Table/Row/Cell with borders; row highlight implemented via `.highlight_style(...)`
  - Layout for table+help: `Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])`
  - Terminal lifecycle implemented using crossterm alternate screen + raw mode; input via `crossterm::event`
  - Note: Initial TUI is in `crates/agency/src/tui/mod.rs:1`
- [ ] Protocol additions (`crates/agency/src/pty/protocol.rs`)
  - [ ] Add `SubscribeEvents`, `NotifyTasksChanged`, `TasksChanged`, `SessionsChanged` variants with serde derives
- [ ] Daemon subscribers (`crates/agency/src/pty/daemon.rs`)
  - [ ] Track `Vec<Subscriber { project: ProjectKey, control: D2CControlChannel }>`
  - [ ] Handle `SubscribeEvents` first frame: register; writer thread; keep connection open
  - [ ] Broadcast points:
    - After open/join: `SessionsChanged` with `registry.list_sessions(Some(&project))`
    - In exit poller: send `SessionsChanged` for affected project
    - On stop session/task: send `SessionsChanged`
    - On `NotifyTasksChanged`: send `TasksChanged { project }`
- [x] TUI module (`crates/agency/src/tui/mod.rs`)
  - [x] If stdout is not TTY: `log_info!` help and return
  - [x] Build rows from `list_tasks()` + session map; preserve selection
  - [x] Render bordered table and centered help bar:
    - `Up/Down j/k Select | Enter Edit/Attach | q Quit` with foreground color
  - [ ] Input overlay titled “New slug”; integrate `normalize_and_validate_slug`
  - [x] Input handling:
    - Enter: join session or open editor on Draft (opens markdown task file); re-init terminal after action and return to TUI
    - q/Esc: exit
  - [ ] Subscribe reader thread:
    - Open `SubscribeEvents { project }`; forward `TasksChanged`/`SessionsChanged` into UI loop
    - On `TasksChanged`: reload tasks from FS; rebuild rows
    - On `SessionsChanged`: update session map and rows
  - Note: Status colors match `ps` (Running=green, Exited=red, Draft=yellow)
- [x] CLI wiring (`crates/agency/src/lib.rs`)
  - [x] Add `Tui {}` subcommand; default to TUI on `None` when TTY; else print help
  - Note: default launch when TTY; non‑TTY prints help
- [ ] Tests (unit)
  - [ ] Pure helpers: row building, selection movement, status mapping
  - [ ] Reset logic: branch/worktree pruning and task file retention
- [ ] Quality
  - [x] `just check` and fix clippy warnings
  - [ ] `just fmt`

## Questions
1) Enter on Draft: always open editor; S/N explicitly start sessions (assumed)
2) Non-TTY help: single `log_info!` line vs. multi-line banner (assumed single line)
3) Reset (R): delete branch/worktree + stop sessions without fast-forward/rebase (assumed direct delete)
4) N flow: block until editor exits, then start+attach (assumed)
