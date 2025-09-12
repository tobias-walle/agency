# PLAN-3: Modular StdinHandler with KeyBinding for attach input

Date: 2025-09-10

## Context

Interactive attach currently scans raw stdin bytes inline to detect a configurable detach sequence.
There are reports of "skipped keys" due to partial matches held across chunk boundaries without timely flushing.
We also want to generalize the logic to support future keyboard sequences beyond detach.

## Objective

Introduce a reusable, testable `StdinHandler` that:
- Accepts configurable `KeyBinding` sequences (e.g., detach) at creation.
- Processes raw stdin chunks, emitting passthrough bytes and matched binding events.
- Correctly handles sequence detection across chunk boundaries without data loss.
- Prefers longest matches for overlapping/prefix bindings.
- Surfaces debug logs to a file without altering stdout behavior.

## Design

### Data model

- `KeyBinding`:
  - `id: String` — stable identifier, e.g., "detach".
  - `bytes: Vec<u8>` — exact byte sequence (e.g., `[0x10, 0x11]` for `Ctrl-P, Ctrl-Q`).
  - `consume: bool` — whether matched bytes are suppressed from passthrough.

- `StdinHandler`:
  - Holds `bindings: Vec<KeyBinding>` and derived `max_seq_len`.
  - Maintains a small `pending` window to track cross-chunk prefixes (bounded by `max_seq_len - 1`).
  - API:
    - `new(bindings: Vec<KeyBinding>) -> Self`
    - `process_chunk(&mut self, chunk: &[u8]) -> (Vec<u8>, Vec<String>)` — returns passthrough bytes and matched binding IDs.
    - `flush_pending(&mut self) -> Vec<u8>` — flushes withheld partials at EOF.
  - Matching policy:
    - For each byte, extend the pending window and check for binding matches.
    - Prefer the longest matching binding when multiple match the current suffix.
    - On match: emit binding ID; suppress bytes if `consume`, otherwise pass them through; reset the matched portion.
    - If `pending.len()` exceeds `max_seq_len - 1`, flush earliest bytes into passthrough to keep withholding minimal.
    - At end-of-chunk, ensure any non-matching pending bytes are flushed to avoid latency/skips.

- Reader integration:
  - `enum Msg { Data(Vec<u8>), Binding(String) }`.
  - `spawn_stdin_reader(bindings: Vec<KeyBinding>, tx: std::sync::mpsc::Sender<Msg>) -> std::thread::JoinHandle<()>`:
    - Reads raw bytes from `stdin`.
    - Uses `StdinHandler` and sends `Msg::Data` and `Msg::Binding(id)` as appropriate.
    - On EOF, sends `Data` for `flush_pending()` if non-empty.

- Debug logging (CLI):
  - Always-on tracing-based logging to `./.agency/cli.logs.jsonl` (JSON Lines), similar to the daemon setup.
  - Initialize a non-blocking file appender at process start; create `./.agency` if missing; append mode only.
  - Use `tracing`/`tracing-subscriber` to emit structured logs; prefer `trace!` for high-volume byte-level details and `debug!` for summaries.
  - Do not write debug logs to stdout/stderr; if file initialization fails, skip logging rather than interleaving with the terminal.

### Integration points

- File: `apps/agency/src/main.rs` — initialize the CLI tracing subscriber that writes JSONL to `./.agency/cli.logs.jsonl` (non-blocking appender, append mode, ensure directory exists).
- File: `crates/cli/src/stdin_handler.rs` — implement `KeyBinding`, `StdinHandler`, `Msg`, `spawn_stdin_reader`, and unit tests. Use `tracing` macros for all debug output.
- File: `crates/cli/src/lib.rs` — replace inline stdin scanning in `attach_interactive()` with `spawn_stdin_reader`:
  - Construct binding: `KeyBinding { id: "detach".into(), bytes: detach_seq, consume: true }`.
  - Set `want_detach` on receiving `Msg::Binding("detach")`.
  - Keep resize thread and RPC session logic unchanged.
  - Retain current input-forwarding state for now; re-enable later in a follow-up.

## Testing strategy

- Unit tests (`crates/cli/src/stdin_handler.rs` under `#[cfg(test)]`):
  - Single-key binding consumed (`Ctrl-Q`) with surrounding noise.
  - Multi-key binding across chunks (`Ctrl-P, Ctrl-Q`).
  - Overlapping/prefix bindings — ensure longest-match preference.
  - Partial at end-of-chunk — ensure pending bytes are flushed (previous bug fix).
  - EOF flush returns any remaining pending bytes.

- Integration test (`crates/cli/tests/attach_fast_input.rs`):
  - Setup repo + daemon; run `init`, `new`, `start`, then `attach`.
  - Write many tiny chunks rapidly (1–3 bytes) with near-detach prefixes that don’t complete until the end.
  - Finally send the full detach sequence; assert success, "Attached. Detach:" in stdout, and "detached" in stderr.
  - Do not rely on debug logs for assertions.

## Acceptance criteria

- `just check` and `just test` pass.
- `StdinHandler` unit tests validate cross-chunk behavior and longest-match policy.
- Integration test reliably reproduces fast small-chunk input without skipped keys.
- CLI emits no debug logs to stdout/stderr; all logs are always appended to `./.agency/cli.logs.jsonl`.
- `attach_interactive()` uses the new `stdin_handler` cleanly, improving readability and testability.

## Tasks (execution checklist)

1. Initialize tracing for the CLI: JSONL file appender to `./.agency/cli.logs.jsonl` (non-blocking, append; ensure directory exists) in `apps/agency/src/main.rs`.
2. Create `stdin_handler.rs` with `KeyBinding`, `StdinHandler`, `Msg`, `spawn_stdin_reader`, and use `tracing` for debug logs.
3. Update `crates/cli/src/lib.rs` to use `spawn_stdin_reader` and handle `Binding("detach")`.
4. Add unit tests in `stdin_handler.rs` for sequences and chunking behavior (including end-of-chunk flush).
5. Add `crates/cli/tests/attach_fast_input.rs` to simulate very fast small-chunk inputs.
6. TDD: run tests to see expected failures before implementation; iterate until green.
7. Run the full test suite; verify stability across platforms.
8. Follow-up (separate PR): re-enable `pty_input` and adjust E2E assertions to rely on real PTY output.

## Risks and mitigations

- Overhead from scanning logic:
  - Keep `bindings` small; use simple suffix checks and longest-match only when needed.
- Flaky timing in integration test:
  - Use small sleeps only if necessary; assert robust substrings; tolerate minor delays.
- Log file growth:
  - Start without rotation for simplicity; add rotation with `tracing-appender` in a future iteration if needed.
- Future binding conflicts:
  - Enforce clear IDs and document precedence (longest match, then definition order).
