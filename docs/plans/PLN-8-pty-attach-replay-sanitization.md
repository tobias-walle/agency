# PLAN-8: PTY Attach Replay Sanitization + TUI-Friendly Attach

This plan fixes broken screens after `agency attach` by keeping sanitized replay for non-TUIs and handling TUIs specially by skipping replay and forcing an immediate redraw.
It consolidates all details so an engineer can implement without further research.

## Goals

- Always-on sanitized replay on attach for non-TUIs, preserving ANSI colors and formatting.
- Replay starts at a safe boundary and never begins inside an escape sequence.
- Avoid CR-only "overprint" artifacts in replay while leaving live streaming unmodified.
- Keep existing CLI `--no-replay` for debugging flows.
- TUI-friendly attach: when a TUI is detected, skip replay and force an immediate redraw via a minimal resize "jiggle".
- Do not introduce any new CLI flags or config switches.

## Non-Goals

- Do not perform full terminal emulation.
- Do not alter live streaming output.
- Do not add new flags or config toggles for this behavior.
- Do not reset terminals/apps via RIS/DECSTR; keep changes minimally invasive.

## Current Behavior (implemented)

- Server-side replay sanitization on `pty.attach` with conservative head/tail trimming and isolated `\r` normalization.
- CLI flag `--no-replay` is wired through RPC as `replay: Option<bool> = Some(true)` by default.
- Initial `pty.resize` is sent by the daemon after attach and the CLI watches terminal resizes and forwards them.

## Problem Statement (investigation)

- Full-screen TUIs (e.g., lazygit) render using the alternate screen and cursor-positioning + clears.
- The attach-time replay tail is usually not a coherent "frame" and can place text without prerequisite clears/positioning, leaving a broken view.
- A manual resize triggers a TUI redraw and fixes the screen.
- With `--no-replay`, the initial screen is empty until a resize occurs, confirming that a redraw is required post-attach.

## Design: TUI-aware attach + redraw jiggle

- Detect alt-screen usage by scanning the PTY output stream in the reader thread and tracking state.
- Enter alt-screen sequences to detect:
  - `ESC [ ? 1049 h`, `ESC [ ? 1047 h`, `ESC [ ? 47 h`.
- Leave alt-screen sequences to detect:
  - `ESC [ ? 1049 l`, `ESC [ ? 1047 l`, `ESC [ ? 47 l`.
- Maintain a small rolling lookbehind buffer (up to 8 bytes) to catch sequences split across read boundaries.
- Attach behavior:
  - If `replay == false` OR alt-screen is currently active, skip replay prefill and set an empty outbox.
  - Immediately force a redraw via a minimal resize jiggle:
    - If `cols > 1`: `resize(rows, cols - 1)` then `resize(rows, cols)`.
    - Else if `rows > 1`: `resize(rows - 1, cols)` then `resize(rows, cols)`.
    - Else: single `resize(rows, cols)` (degenerate case).
- Non-TUI behavior remains unchanged:
  - Keep sanitized replay for non-TUIs for a readable scrollback experience.
  - Keep live streaming byte-accurate and unsanitized.

## Implementation Steps

1) Core PTY adapter (`crates/core/src/adapters/pty.rs`).
- Extend `PtySession` with:
  - `alt_screen_active: AtomicBool`.
  - `alt_detect_tail: Mutex<Vec<u8>>` (last up to 8 bytes, initially empty).
- Reader thread changes (inside the loop that reads into `tmp`).
  - Build `scan = alt_detect_tail[..] + data[..]` (truncate lookbehind to <= 8 bytes).
  - Search `scan` for the enter/leave alt-screen sequences above and set `alt_screen_active` accordingly.
  - After scanning, store the last up to 8 bytes of `scan` into `alt_detect_tail`.
  - Emit logs on transitions:
    - `pty_alt_screen_on` | `pty_alt_screen_off` with `task_id`.
- `attach(project_root, task_id, prefill: bool) -> String` changes.
  - Compute `effective_prefill = prefill && !sess.alt_screen_active.load(Ordering::SeqCst)`.
  - If `effective_prefill` is true, keep current sanitized tail behavior (limit by `ATTACH_REPLAY_EMIT_BYTES`).
  - Else set `outbox = Some(Vec::new())` (no replay).
  - Keep `cv.notify_all()` to wake pending readers.
- Add `pub fn jiggle_resize(attachment_id: &str, rows: u16, cols: u16) -> anyhow::Result<()>`.
  - Implement two-step resize using minimal 1-row/1-col delta as described above.

2) Daemon attach handler (`crates/core/src/daemon/mod.rs`).
- In `"pty.attach"` handler, after `attach_id = pty::attach(...)`:
  - Replace the single `resize(&attach_id, p.rows, p.cols)` with `let _ = crate::adapters::pty::jiggle_resize(&attach_id, p.rows, p.cols);`.
- Add a log `pty_attach_jiggle_resize` with the chosen jiggle sizes.

3) CLI (`crates/cli/src/lib.rs`).
- No functional changes required.
- Keep the resize watcher; it complements the initial jiggle.

4) RPC DTOs (`crates/core/src/rpc/mod.rs`).
- No changes required (`PtyAttachParams { rows, cols, replay }` stays as-is).

5) Config (`crates/core/src/config/mod.rs`).
- No changes required.
- Do not add toggles/flags for this behavior.

## Sanitized Replay Details (non-TUI)

- The sanitizer for replay tail does the following (unchanged, for non-TUI attach):
  - Tail window: start from `max(len - ATTACH_REPLAY_BYTES, 0)`.
  - Align head to a safe boundary: first `\n`, `ESC (0x1B)`, or printable ASCII (>= 0x20 and != 0x7F)`.
  - Convert isolated `\r` (not followed by `\n`) to `\n`.
  - Truncate dangling tail if ending mid-escape (e.g., `ESC`, `ESC[` without final in `0x40..0x7E`).
  - Preserve complete ANSI/SGR sequences; do not rewrite or recolor.

## TDD

- Unit tests in `crates/core/src/adapters/pty.rs`.
  - Keep existing sanitizer tests.
  - Add `alt_screen_detection_enters_and_leaves`:
    - Feed chunks simulating `...\x1b[?1049h...` then `...\x1b[?1049l...`, including a case split across the chunk boundary via `alt_detect_tail`.
    - Assert `alt_screen_active` toggles `true` then `false`.
- Core integration tests in `crates/core/tests/pty.rs`.
  - `attach_skips_replay_when_alt_screen_active`:
    - Start session; write data containing `\x1b[?1049h` to mark alt-screen active.
    - Detach, then attach with default replay (true).
    - Immediately read; assert no replayed history (empty or only new live bytes).
  - `attach_performs_resize_jiggle`:
    - Attach and assert via logs that `pty_attach_jiggle_resize` was emitted.
    - Alternatively, call `jiggle_resize` in isolation (smoke test: no error with boundary sizes).
- CLI E2E notes (under `crates/cli/tests`).
  - Keep `attach_no_replay.rs` as-is.
  - Manual validation recommended for TUIs: jiggle should cause immediate redraw on attach.

## Observability

- Logs:
  - `pty_alt_screen_on`, `pty_alt_screen_off` with `task_id`.
  - `pty_attach_jiggle_resize` with `rows`, `cols`, and intermediate jiggle sizes.
  - Keep `pty_attach_replay_prefill` with `replay_bytes`, `dropped_head`, `dropped_tail` for non-TUI attaches.

## Acceptance Criteria

- For non-TUIs, re-attach shows clean, colored replayed tail (sanitized) with no garbling.
- For TUIs (alt-screen active), re-attach does not replay history and the screen is correct without manual resize (redraw triggered by jiggle).
- `--no-replay` no longer yields an empty screen for TUIs; the jiggle causes an immediate redraw.
- All tests pass: `just check` and `just test` green.

## File/Function Touchpoints

- `crates/core/src/adapters/pty.rs`.
  - Add fields to `PtySession`: `alt_screen_active`, `alt_detect_tail`.
  - Reader thread: detect `\x1b[?1049{h|l}`, `\x1b[?1047{h|l}`, `\x1b[?47{h|l}` and update `alt_screen_active`.
  - `attach(...)`: compute `effective_prefill` and gate replay accordingly.
  - New: `jiggle_resize(attachment_id, rows, cols)`.
- `crates/core/src/daemon/mod.rs`.
  - In `"pty.attach"`, call `jiggle_resize(...)` instead of a single `resize(...)`.
- `crates/cli/src/lib.rs`.
  - No change; existing resize watcher remains.
- `crates/core/src/rpc/mod.rs`.
  - No change.
- `crates/core/src/config/mod.rs`.
  - No change.

## Progress & Learnings (updated)

- Sanitizer is implemented server-side and verified via unit and integration tests.
- Investigation shows TUIs require a redraw on re-attach rather than replay; `--no-replay` confirms the need for a redraw.
- Design chosen: skip replay when alt-screen is active and force redraw via jiggle; this avoids new flags and keeps behavior robust for TUIs.
