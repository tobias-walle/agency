# PLAN: Return Full Scrollback On Attach

Make reattach include the full scrollback in the Welcome snapshot so terminals can scroll back after reattach.

## Goals
- Return full scrollback (not just the visible screen) in the attach Welcome snapshot.
- Preserve terminal-native scrollback after detach/reattach.
- Keep protocol and client behavior stable and compatible.
- Maintain reasonable performance and memory usage.

## Out of Scope
- Multi-session multiplexing or concurrent client changes.
- TUI alternate-screen handling and its scrollback behavior.
- Configurable scrollback size and persistent transcript storage across daemon restarts.

## Current Behavior
- The session parses PTY output using `vt100::Parser` with scrollback enabled:
  - `crates/agency/src/pty/session.rs:57` initializes with `vt100::Parser::new(rows, cols, 10_000)`.
- Snapshot used on attach and restart returns only the current screen:
  - `crates/agency/src/pty/session.rs:220` `snapshot()` uses `screen.contents_formatted()` and returns `(ansi, (rows, cols))`.
- Daemon sends a Welcome with this snapshot:
  - `crates/agency/src/pty/daemon.rs:338` and `crates/agency/src/pty/daemon.rs:470` send `D2CControl::Welcome { ansi, rows, cols }`.
- Client prints the Welcome `ansi` directly to stdout:
  - `crates/agency/src/pty/client.rs:67` handshake writes `ansi` to stdout.
- Outcome: On reattach, only the current view is printed; prior scrollback isn’t re-emitted, so the terminal has nothing to scroll.

## Solution
- Change the attach snapshot to include the entire scrollback buffer, not just the visible screen.
- Prefer using `vt100` APIs to render the full scrollback into ANSI in one flush, leveraging the parser’s existing buffer.
- Keep the streaming of live PTY output unchanged; only expand the initial `Welcome.ansi`.
- Use the same approach for `RestartSession` Welcome snapshots.
- If `vt100` lacks a direct “full scrollback” renderer, add a lightweight raw transcript ring buffer in `Session` as a fallback and emit that on attach before resuming live output.
- Perform a full reset in the client before printing the Welcome ANSI to avoid interleaving histories.

## Architecture
- `crates/agency/src/pty/session.rs`
  - Add `snapshot_full_scrollback() -> (Vec<u8>, (u16, u16))` leveraging `vt100::Parser` or a transcript buffer to render the entire buffer.
  - Keep `snapshot()` for “current screen” (if still needed by callers); new daemon paths will call the full version.
- `crates/agency/src/pty/registry.rs`
  - Update `snapshot(session_id)` to call `snapshot_full_scrollback()` on the `Session`.
- `crates/agency/src/pty/daemon.rs`
  - `attach_to_session()` and `RestartSession` flows use full scrollback snapshot for `D2CControl::Welcome`.
- `crates/agency/src/pty/protocol.rs`
  - No schema changes; `D2CControl::Welcome { ansi }` continues to carry ANSI bytes (now expanded).
- `crates/agency/src/pty/client.rs`
  - Before writing `Welcome.ansi`, perform a full terminal reset that clears the scrollback to avoid interleaving histories.
- Tests
  - `crates/agency/tests/pty_attach_scrollback.rs` (new): start daemon + session, produce lines, detach, reattach, assert Welcome includes early lines not visible on the current screen.

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)

1. [ ] Validate vt100 API for full scrollback rendering
   - Inspect `vt100::Screen` methods to render entire scrollback. If only `contents_formatted()` exists, confirm it includes scrollback; otherwise, find a range-based API or iterate scrollback lines.
   - Fallback: design a small raw transcript ring buffer in `Session` (VecDeque of byte chunks, capped) that records output in `start_read_pump()` and is emitted on attach.

2. [ ] Add `snapshot_full_scrollback()` in `crates/agency/src/pty/session.rs`
   - Mirror `snapshot()` but render all buffered content via transcript. Return `(ansi, (rows, cols))`.

3. [ ] Update the registry snapshot to call the new method
   - `crates/agency/src/pty/registry.rs:297` change `snapshot()` to use `session.snapshot_full_scrollback()`.

4. [ ] Use full snapshot in daemon attach/restart flows
   - `crates/agency/src/pty/daemon.rs:338`: replace `snapshot(session_id)` usage with the full snapshot.
   - `crates/agency/src/pty/daemon.rs:470`: same change for `RestartSession` Welcome.

5. [ ] Clear scrollback before printing Welcome in the client
   - Add `utils::term::clear_terminal_scrollback()` that sends `CSI 3J; CSI 2J; CSI H`.
   - Call it at the start of `handshake()` before writing `Welcome.ansi`.

6. [ ] Add PTY attach scrollback test
   - `crates/agency/tests/pty_attach_scrollback.rs`:
     - Ensure fake agent writes multiple lines; detach; reattach.
     - Assert the Welcome `ansi` contains an early unique marker line that would be off-screen without full scrollback.
     - Use `expectrl` and helper functions from `pty_helpers`.

7. [ ] Run checks and tests
   - `just check` and address lints per project style.
   - `just test` to validate behavior.
   - `just fmt` for formatting.

8. [ ] Performance sanity
   - Confirm large scrollback ANSI emission is acceptable with a capped transcript (e.g., 8 MiB).
   - If needed, adjust cap or make it configurable in a later plan.

## Questions
1. Should we always send the full scrollback on every attach, including the initial first attach? Default: Yes — consistent behavior and better UX.
2. Is the 10,000-line `vt100` scrollback size acceptable for memory and initial ANSI payload size? Default: Yes — keep as-is; revisit if performance issues arise.
3. Do we need a configuration knob for scrollback lines or whether to emit full scrollback? Default: Not now — defer config to a later plan.
4. If `vt100` doesn’t support full scrollback rendering directly, is a raw transcript buffer acceptable as a fallback? Default: Yes — implement a simple capped transcript in `Session`.
5. Should restart Welcome also include full scrollback so scrollback persists after a shell restart? Default: Yes — consistent attach behavior after restart.
6. Any concern about double-printing content (e.g., visible screen lines already exist), leading to duplication? Default: Acceptable — users expect full history on attach; ANSI re-emission populates terminal scrollback intentionally.

