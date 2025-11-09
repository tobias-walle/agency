# PLAN: TUI Command Log and Snappy Actions

Add a framed Command Log panel to the TUI and keep the UI responsive by running non-interactive actions in the background without leaving the alternate screen. Interactive flows (attach/editor) still temporarily leave the TUI.

## Goals
- Keep the TUI open for non-interactive actions; avoid tearing down alt screen
- Show a framed Command Log under the task table; help bar stays bottom-most
- Render logs with the same coloring as CLI output; include a leading “> agency …” line per action
- Run non-interactive actions on background threads so the TUI remains responsive

## Out of scope
- Any on-disk persistence or telemetry of the command log history
- Streaming interactive PTY/editor output inside the TUI
- Changing attach/editor flows (they continue to leave and re-enter the TUI)

## Current Behavior
- TUI initialization and drawing happen in `crates/agency/src/tui/mod.rs:58`, entering alt screen at `:67` and drawing table+help in `:95`.
- Many actions close the TUI to let stdout logs print, then re-enter the alt screen:
  - Restore/reinit helpers: `crates/agency/src/tui/mod.rs:319`, `:327`
  - Key handlers: Enter, Start, Stop, Delete, Reset, New submit (`:204-303`)
- CLI logs are emitted by macros in `crates/agency/src/utils/log.rs:40-63` and printed via `anstream`/ANSI tokens styled by `owo_colors`.

## Solution
- Global log sink routes CLI macro output into the TUI when set; falls back to `stdout/stderr` when not set.
- Log events:
  - `Command(String)` for a leading command line (rendered as `"> ..."` in gray)
  - `Line { level, ansi }` with original ANSI text; converted to Ratatui spans for consistent coloring
- Command Log panel sits between Tasks and Help, fixed height, framed. It renders only the visible lines (auto-scroll by slicing to the last N lines).
- Non-interactive actions run in background threads and keep the alt screen:
  - Stop (S): requests daemon stop + logs a confirmation line; table refresh follows notify
  - Reset (R): stop + prune worktree + delete branch + notify; logs confirmations
  - Delete (X): force delete (no stdin); logs only a single success line
- Interactive actions (attach/editor) temporarily leave the TUI and re-enter; sink is cleared before and restored after to print interactive output to stdout.

## Architecture
- `crates/agency/src/utils/log.rs`
  - Added `LogLevel`, `LogEvent`, global `SINK: Mutex<Option<Sender<LogEvent>>>`
  - `set_log_sink`, `clear_log_sink`, and `emit` used by `log_*` macros
- `crates/agency/src/tui/colors.rs`
  - ANSI-to-Ratatui conversion via `anstyle-parse` custom `Perform` handling SGR; maps colors and effects
- `crates/agency/src/tui/mod.rs`
  - `AppState` holds `cmd_log` and push+truncate helper
  - Layout uses three rows: `[Fill(1), Length(5), Length(1)]`
  - Rendering slices to the last N lines so the newest content is always visible; command line rendered in gray
  - Key handlers:
    - `S`: background stop + info confirmation
    - `R`: background reset + confirmations (prune/delete)
    - `X`: background `rm::run_force` (no stdin)
    - `n`: interactive editor open (leave alt screen, then re-enter)
    - `s`, `Enter`, `N`: interactive attach flows unchanged
- `crates/agency/src/commands/rm.rs`
  - Added `run_force(ctx, ident)` to remove without confirmation and emit a single condensed success line
- Tests
  - Log sink routing tests for `utils/log.rs`
  - Basic ANSI parsing tests in `tui/colors.rs`
  - Existing TUI tests remain

## Detailed Plan
- [x] Add log sink types and API in `utils/log.rs`
  - [x] `LogLevel`, `LogEvent`, `SINK`, `set_log_sink/clear_log_sink`, `emit(level, text)`
  - [x] Macros route through `emit(...)` and preserve ANSI

- [x] Tests for `utils/log.rs`
  - [x] Sink routing emits `LogEvent::Line` with correct level and ANSI
  - [x] No panic without sink

- [x] Extend TUI state and layout
  - [x] `cmd_log` with push+truncate
  - [x] 3-row layout; framed table and Command Log; help bottom
  - [x] Command Log: gray `"> ..."` lines; ANSI spans
  - [x] Render only visible lines for vertical autoscroll

- [x] ANSI conversion via `anstyle-parse` (tui/colors.rs)
  - [x] Added dependency and module
  - [x] Tests for basic colored and reset lines

- [x] Wire sink in `ui_loop`
  - [x] Channel set + drain into `cmd_log`
  - [x] Clear sink on exit

- [x] Background non-interactive actions
  - [x] `S` and `R` run in background and log confirmations
  - [x] `X` uses `rm::run_force` and logs single-line confirmation
  - [x] `n` interactive editor open without overlay; `s`, `Enter`, `N` unchanged

- [x] Validation
  - [x] `just check`, `just test`, manual TUI run

## Questions
1) Command Log height default 5 rows. OK? Assumed: Yes.
2) ANSI parsing: current mapping covers standard and bright colors + 256 palette; OK?
3) Background actions: allow overlapping or add a simple guard? Assumed: overlapping is acceptable for now.
