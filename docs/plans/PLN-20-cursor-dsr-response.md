# PLAN: Handle Cursor DSR Requests
Ensure background sessions answer CSI cursor-position probes so agents keep running.

## Goals
- Detect PTY output sequences requesting cursor position (`CSI 6 n` / `CSI ? 6 n`).
- Respond with the correct cursor report even when no client is attached.
- Cover the detector logic with focused tests.

## Out of scope
- Changes to attach/start CLI flows.
- Broader PTY protocol adjustments or additional ANSI handling.
- TUI or logging modifications unrelated to cursor reporting.

## Current Behavior
- `crates/agency/src/commands/start.rs:17` opens a session, waits for `Welcome`, then detaches immediately.
- `crates/agency/src/pty/session.rs:78` spawns a read pump that forwards PTY output but never answers device-status queries.
- Codex sends `CSI 6 n` on launch; with no attached client the request goes unanswered, so the agent exits early.

## Solution
- Add a lightweight detector in the PTY read pump to recognize cursor DSR requests across chunk boundaries.
- After updating the `vt100::Parser`, fetch the current cursor position and inject the ANSI cursor report (`ESC[{row};{col}R`) back to the PTY writer for each request seen.
- Unit-test the detector to ensure it handles plain and DEC-private variants and ignores unrelated sequences.

## Architecture
- `crates/agency/src/pty/session.rs`
  - Extend `Session::start_read_pump` to track DSR requests and send responses via the existing PTY writer.
  - Introduce an internal `CursorRequestDetector` helper with associated tests (`#[cfg(test)]`).
- (No new files; changes stay within `session.rs`.)

## Detailed Plan
- [ ] Add a `CursorRequestDetector` struct in `session.rs` (near other helpers) that consumes bytes incrementally and signals when a `CSI 6 n` or `CSI ? 6 n` request is complete. Include unit tests covering matching and non-matching sequences.
- [ ] Update `Session::start_read_pump`:
  - [ ] Clone the PTY writer into the read thread.
  - [ ] Instantiate the detector and feed each output chunk, counting detected requests.
  - [ ] After `vt100` processes the chunk, capture the cursor position, then (outside the parser lock) send the ANSI response back through the PTY writer for each pending request. Handle write errors with a warning log.
- [ ] Run `just check` (or `cargo test`) to ensure the new logic and tests build cleanly; fix any issues.

## Questions
- (none)
