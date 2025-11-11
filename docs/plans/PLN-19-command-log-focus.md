# PLAN: TUI command log focus and scrolling
Expand command log to 5 content lines and add focus switching between Tasks and Command Log with 1/2; enable j/k and arrows to scroll the log when focused.

## Goals
- Increase Command Log to show 5 content lines.
- Add focus switching: 1 selects Tasks, 2 selects Command Log.
- When Command Log focused, j/k and arrows scroll the log.
- Keep existing task navigation and actions unchanged.

## Out of scope
- Page up/down or half-page scrolling.
- Mouse interactions.
- Persisting focus or scroll across runs.
- Adding a third pane or resizing by drag.
- Daemon/log pipeline changes.

## Current Behavior
- The TUI draws three vertical regions: Tasks table, Command Log, Help bar in `crates/agency/src/tui/mod.rs:210`.
- Command Log height is fixed at 5 rows total; inner content excludes borders (shows effectively 3 lines) in `crates/agency/src/tui/mod.rs:219`.
- Log rendering always auto-scrolls to the latest lines (`start = total_lines.saturating_sub(content_h)`) in `crates/agency/src/tui/mod.rs:274`.
- Mode handling includes List/InputSlug/SelectMenu (`crates/agency/src/tui/mod.rs:609`).
- In List mode, Up/Down and j/k move the selected task; Enter edits/attaches; various action keys exist (`crates/agency/src/tui/mod.rs:350`).
- There is no notion of pane focus; log is not selectable or scrollable.
- Help shows navigation and action tips (`crates/agency/src/tui/mod.rs:195`).

## Solution
- Introduce a focus concept with two panes: Tasks and Log.
- Add AppState fields for current focus and a log scroll offset.
- Change layout to set Command Log height to 7 rows (5 content + 2 borders).
- Modify key handling in List mode:
  - 1 -> focus Tasks
  - 2 -> focus Log
  - If focus Tasks: existing selection handling remains (Up/Down/j/k move selection).
  - If focus Log: Up/Down/j/k adjust `log_scroll` (Up increases offset; Down decreases; clamp at 0 to stick to bottom).
- Render Command Log using `log_scroll` when focused; otherwise stick to auto-scroll (latest).
- Visual indicator for focus via title color tint (no help bar changes).

## Architecture
- crates/agency/src/tui/mod.rs
  - AppState
    - + focus: Focus
    - + log_scroll: usize
  - enum Focus { Tasks, Log }
  - Drawing
    - Change command log block height to 7 rows.
    - Compute visible log window using `log_scroll` when focused.
    - Tint active pane title cyan.
  - Key handling (Mode::List)
    - Add handlers for '1' and '2'.
    - Route Up/Down/j/k to tasks or log based on focus.
  - Tests (unit tests at bottom of same file)
    - Add helper `compute_log_start(total, content_h, log_scroll)` and tests.

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
- [ ] Add Focus enum and AppState fields
  - Add `enum Focus { Tasks, Log }` with Default=Tasks
  - AppState: `focus: Focus` default Tasks; `log_scroll: usize` default 0
- [ ] Increase log height to 7 rows
  - In layout construction (`crates/agency/src/tui/mod.rs:210`), change `Constraint::Length(5)` to `Constraint::Length(7)` so content becomes 5 lines
- [ ] Adjust log rendering to support manual scroll
  - Compute `content_h = rects[1].height - 2`
  - If `state.focus == Focus::Log`, set `start = total_lines.saturating_sub(content_h + state.log_scroll)`
  - Else keep auto-scroll with `start = total_lines.saturating_sub(content_h)`
- [ ] Add keybindings to switch focus
  - In `Mode::List` match, add `KeyCode::Char('1') => state.focus = Focus::Tasks; state.log_scroll = 0`
  - Add `KeyCode::Char('2') => state.focus = Focus::Log`
- [ ] Implement log scrolling in List mode when focused
  - If `Focus::Log` and `Up` or `'k'`: `state.log_scroll = state.log_scroll.saturating_add(1)`
  - If `Focus::Log` and `Down` or `'j'`: `state.log_scroll = state.log_scroll.saturating_sub(1)`
  - If `Focus::Tasks`: keep existing task selection logic
- [ ] Visual focus indicator
  - For Tasks and Log: tint title cyan when focused
- [ ] Add unit tests in `crates/agency/src/tui/mod.rs`
  - Test `compute_log_start(total_lines, content_h, log_scroll)` boundaries
- [ ] Run `just check` and fix warnings
- [ ] Run `cargo fmt`

## Questions
1) Should 1/2 focus switching only work in List mode (not in overlays like New Task input or SelectMenu)?
- Default: Yes, only in List mode.

2) For “5 lines of content”, is it correct to set the Command Log block height to 7 (5 content + 2 borders)?
- Default: Yes, use 7 total rows.

3) When the log is focused and new log lines arrive, should the view stick to the bottom only when `log_scroll == 0` and otherwise preserve the current offset?
- Default: Yes, preserve manual scroll unless at bottom.

4) Do you want a visual cue for the active pane (e.g., cyan title)?
- Default: Yes, tint the active panel title cyan.

5) Should Down/j at bottom while focused on Log clamp at bottom (no-op), and Up/k keep scrolling into older lines with no hard limit beyond available history?
- Default: Yes, clamp at bottom, allow scrolling up to the earliest line.

6) Don't add a new hint or update the help bar.
- Confirmed: Keep help bar unchanged.

