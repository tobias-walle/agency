# PLN-4: PTY resize without consuming stdin (cross-platform)

## Context

Fast input bursts occasionally drop characters while attached to a task.
Investigation shows our resize handling thread calls `crossterm::event::read()` which can consume stdin key events, racing with the CLI stdin reader.
This leads to intermittent missing bytes during fast typing or when terminal events are flowing.

We will remove stdin consumption from the resize path by polling terminal size directly, preserving cross-platform support.
We will also strengthen tests to reproduce and guard against this regression.

## Goals

- Prevent the resize handler from consuming stdin events.
- Keep behavior cross-platform (macOS, Linux, Windows) without OS-specific signals.
- Preserve current detach key behavior (including multi-key sequences).
- Validate end-to-end with TDD that fast bursts are fully echoed.

## Non-goals (Phase 2 follow-up)

- Binary-safe PTY data over RPC (move from `String` to base64 bytes or `Vec<u8>`).
- Multiple concurrent attachments to a single PTY session.

## Design

- Replace the current resize thread that uses `crossterm::event::{poll, read}` with a non-consuming size poller.
- New approach: spawn a thread that periodically calls `crossterm::terminal::size()` (e.g., every 200 ms), compares with the last sent size, and emits `pty.resize` only if changed.
- This approach is portable across platforms supported by `crossterm` and does not read or consume input events.
- Keep the attach loop batching for stdin and long-poll read behavior unchanged.

### Implementation outline

- File: `crates/cli/src/lib.rs`
  - Remove the `crossterm::event`-based resize thread in `attach_interactive`.
  - Add a background thread that:
    - Reads current size via `crossterm::terminal::size()`.
    - If `(rows, cols)` changed since last report, send a message over `mpsc` to the main loop.
    - Sleep for ~200 ms between checks to avoid CPU spin.
  - In the main loop, drain pending resize messages and send `pty.resize` RPCs.

- Tests: `crates/cli/tests/attach_fast_input.rs`
  - Add a new test that sends a rapid burst of ASCII characters and asserts full echo, e.g.:
    - Start/attach to a running task.
    - Write a known burst like `abcdefghijklmnopqrstuvwxyz0123456789` + `\n` in tiny chunks with minimal sleeps.
    - Wait for output; assert the entire burst appears in order in stdout.
  - Keep the existing near-detach-prefix test to ensure binding withholding still works and the final detach triggers.

## TDD Plan

1. Extend `attach_fast_input.rs` with a failing test that asserts full echo of a long burst written in small, rapid chunks (reproduces current flakiness by increasing likelihood of stdin consumption).
2. Implement the non-consuming resize polling (size-only polling thread) and remove `event::read()` usage.
3. Run tests and ensure stability across multiple runs.

## Risks and mitigations

- Polling interval too long: resize might feel slightly delayed.
  - Mitigation: 150–250 ms interval; acceptable UX for interactive shells.
- Platform-specific `size()` behavior differences:
  - `crossterm::terminal::size()` is supported across platforms; fallback remains consistent.
- Additional CPU usage from polling:
  - Sleep-based loop with low overhead; negligible impact.

## Acceptance criteria

- New fast-burst test reliably passes locally and in CI.
- Existing attach/detach tests continue to pass.
- No observed dropped characters during manual testing with long bursts.

## Follow-up (Phase 2)

- Switch PTY RPC payloads to a binary-safe representation (base64 or `Vec<u8>`) and update CLI/daemon accordingly.
- Add tests that include non-UTF-8 control sequences to validate round-trip fidelity.