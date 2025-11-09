# PLAN: Unified child I/O routing and just-in-time interactive switching

Route all child stdout/stderr into the TUI Command Log when active, and switch the terminal only at the exact interactive boundary inside command code without changing how the TUI invokes commands.

## Goals
- Route setup/bootstrap stdout and stderr to the TUI Command Log when the sink is active
- Keep CLI behavior unchanged when no sink is set (inherit stdio)
- Keep the TUI responsive during non-interactive setup; show progress in the log
- Switch the terminal right before interactive apps (EDITOR/attach/open) and restore right after

## Out of scope
- Redesigning PTY session UX or streaming attached session output inside the TUI log
- Changing daemon protocol or TUI layout
- Heuristics to auto-detect interactive needs

## Current Behavior
- TUI installs a global log sink so `log_*` macros route to the Command Log: `crates/agency/src/tui/mod.rs:109`
- TUI clears the sink and flips terminal modes around interactive actions:
  - Enter attach/editor: `crates/agency/src/tui/mod.rs:262`
  - Open worktree: `crates/agency/src/tui/mod.rs:341`
  - New task editor: `crates/agency/src/tui/mod.rs:406`
- Bootstrap runs with inherited stdio and prints into the terminal: `crates/agency/src/utils/bootstrap.rs:104-110`
- Sink currently captures only Agency logs; child stdout/stderr are not routed: `crates/agency/src/utils/log.rs`

## Solution
- Add a single child runner `run_child_process` that auto-routes child I/O when a sink is present:
  - If sink present: pipe stdout and stderr, stream lines to sink (stdout=Info, stderr=Warn), stdin=null
  - If no sink: inherit stdio, preserving CLI behavior
- Introduce a lightweight interactive boundary API:
  - `interactive::scope(|| { ... })` wraps the exact interactive action
  - When registered by the TUI: synchronously request BeginInteractive before the closure and EndInteractive after
  - When not registered (CLI): no-op
- Register handlers in the TUI loop to perform the just-in-time terminal switch at Begin/End; keep sink active at all times
- Keep interactive command invocations from TUI on background threads so the loop can service Begin/End acks
- Wrap interactive boundaries at the source sites so the TUI still calls the same command entrypoints:
  - Editor: wrap `utils::editor::open_path(...)`
  - Attach: wrap `pty_client::run_attach(...)` inside `commands::attach::{run_with_task, run_join_session}`
- Route bootstrap via `run_child_process` so setup logs appear in the Command Log when TUI is active

## Architecture
- New
  - `crates/agency/src/utils/child.rs`
    - `run_child_process(program: &str, args: &[String], cwd: &Path, env: &[(String,String)]) -> anyhow::Result<std::process::ExitStatus>`
  - `crates/agency/src/utils/interactive.rs`
    - `pub enum InteractiveReq { Begin { ack: crossbeam_channel::Sender<()> }, End { ack: crossbeam_channel::Sender<()> } }`
    - `pub fn register_sender(tx: crossbeam_channel::Sender<InteractiveReq>)`
    - `pub fn begin() -> anyhow::Result<()>`, `pub fn end() -> anyhow::Result<()>`
    - `pub fn scope<F, R>(f: F) -> anyhow::Result<R> where F: FnOnce() -> anyhow::Result<R>`
- Modified
  - `crates/agency/src/utils/log.rs`
    - `pub fn is_sink_set() -> bool`
  - `crates/agency/src/utils/bootstrap.rs`
    - `run_bootstrap_cmd` uses `run_child_process` instead of unconditional inherit; optional info preface
  - `crates/agency/src/utils/editor.rs`
    - Wrap spawn with `interactive::scope`
  - `crates/agency/src/commands/attach.rs`
    - Wrap `pty_client::run_attach` inside `interactive::scope` in both `run_with_task` and `run_join_session`
  - `crates/agency/src/tui/mod.rs`
    - Create interactive control channel and register it via `utils::interactive::register_sender`
    - Add `paused: bool` in `AppState` to skip draw/event when paused
    - Handle `InteractiveReq::Begin` -> `restore_terminal`, set paused=true, ack
    - Handle `InteractiveReq::End` -> `reinit_terminal`, set paused=false, ack
    - Remove `clear_log_sink` and `set_log_sink` calls around interactive actions; keep sink always set while TUI runs
    - Ensure interactive actions (enter/editor/open/new-editor) are launched on background threads

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
1. [ ] Add sink presence check
   - File: `crates/agency/src/utils/log.rs`
   - Add `pub fn is_sink_set() -> bool { SINK.lock().is_some() }`
2. [ ] Implement unified child runner
   - File: `crates/agency/src/utils/child.rs`
   - If `is_sink_set()` -> pipe stdout/stderr to sink using two reader threads; stdin=null; return status
   - Else -> spawn with inherited stdio; return status
   - Tests: route both stdout and stderr lines when sink set; succeed without sink
3. [ ] Route bootstrap through child runner
   - File: `crates/agency/src/utils/bootstrap.rs`
   - After argv expansion, build env vec; call `run_child_process(...)`
   - Optional `log_info!("Run bootstrap {}", argv.join(" "))`
4. [ ] Add interactive control API
   - File: `crates/agency/src/utils/interactive.rs`
   - Global sender registration; Begin/End with synchronous ack via bounded(0) channels
   - `scope` ensures `end()` runs even on error via guard
5. [ ] Register and handle interactive requests in TUI
   - File: `crates/agency/src/tui/mod.rs`
   - Create RX/TX; register TX; add `paused` flag; handle Begin/End; skip draw when paused
   - Remove sink clear/set around interactive actions
6. [ ] Wrap interactive boundaries inside command code
   - File: `crates/agency/src/utils/editor.rs` -> wrap `open_path`
   - File: `crates/agency/src/commands/attach.rs` -> wrap `run_attach` in both entrypoints
7. [ ] Ensure TUI spawns interactive actions on threads
   - File: `crates/agency/src/tui/mod.rs` -> Enter/editor/open/new-editor actions run in `std::thread::spawn`
8. [ ] Checks and tests
   - `just check` and `just test`
   - Manual: TUI shows bootstrap logs; switch occurs only right before and after attach/editor; no Agency logs leak to stdout

## Questions
1) stderr mapping: Treat stderr lines as Warn, not Error; emit a single `log_error!` on non-zero exit if helpful
2) Bootstrap exit handling: Log a warn when exit status is non-zero while continuing execution
3) TUI paused behavior: When paused, skip draw/input and tick-sleep to avoid a busy loop; acceptable since interactive program owns the terminal
4) Preface lines: Add `log_info!` like "Run bootstrap ..." so the Command Log shows context
5) Background threads: Cover Enter/editor/open/new-editor; other actions already run on background threads

