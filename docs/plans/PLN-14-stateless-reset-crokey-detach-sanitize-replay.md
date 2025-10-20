# PLN-14: Stateless reset, crokey-based detach, and sanitize replay

Date: 2025-10-19

Improve attach/detach reliability across terminals with a stateless reset, adopt crokey for detach key parsing, and fix replay sanitize to avoid indentation artifacts.

## Goals

- Use `crokey` to parse configured detach key(s), removing custom parsing
- Keep input passthrough; match detach via derived legacy byte sequences
- Implement a stateless, cross-terminal reset using `crossterm` primitives
- Preserve `CR` and avoid aggressive sanitize that shifts prompts on replay
- Strengthen tests for detach, re-attach alignment, and sanitize behavior

## Non Goals

- Implement kitty CSI "u" parsing or enable keyboard enhancement flags
- Track full terminal state or set explicit cursor styles per terminal
- Change agent spawning or PTY session lifecycle beyond reset/replay

## Current Behavior

- Detach parsing uses `ctrl-<char>` -> legacy C0 bytes in `crates/cli/src/util/detach_keys.rs`.
- CLI attach consumes only exact byte sequences via `StdinHandler` in `crates/cli/src/stdin_handler.rs`.
- Reset footer emits hardcoded sequences in `crates/cli/src/term_reset.rs` (alt-screen leave, show cursor, SGR reset, bracketed paste off, mouse modes off, OSC resets, cursor style default).
- Replay sanitization in `crates/core/src/adapters/pty/sanitize.rs` converts isolated `\r` to `\n` and drops certain heads/tails, which can misalign prompts on re-attach.

## Solution

- Adopt `crokey` to parse configured detach keys; remove custom parser
- Derive legacy byte sequences from crokey `KeyCombination` for matching
- Keep stdin passthrough; only consume configured detach sequences
- Replace reset footer with a stateless reset using `crossterm`:
  - Leave alternate screen, show cursor, reset colors, enable line wrap
  - Emit minimal bracketed paste off if not covered by `crossterm`
- Adjust sanitize to preserve `CR` and retain complete ANSI sequences
- Add tests to validate detach, second attach alignment, and sanitize

## Detailed Plan

- [x] Tests first: leverage existing CLI/core tests; add minimal updates
  - Files: `crates/cli/tests/attach_e2e.rs`, `attach_fast_input.rs`, `attach_detach_cases.rs`
  - Added focused tests: case-insensitive env parsing and ignoring non-ctrl-letter combos
  - Verify second attach with fake shell shows no indentation drift (pending)
- [x] Integrate `crokey` for detach parsing
  - Files: `crates/cli/src/util/detach_keys.rs`, `crates/cli/src/commands/attach.rs`
  - Replaced custom parsing with `crokey::parse` + `normalized()` for env/config strings
  - Mapped `KeyCombination` ctrl-letter to legacy C0 byte(s) for binding setup
  - Dependency added via `cargo add -p cli crokey`
- [ ] Keep stdin forwarder; bind only configured legacy sequences
  - Files: `crates/cli/src/stdin_handler.rs`
  - Ensure bindings use bytes derived from crokey; no kitty CSI parsing
- [ ] Implement stateless reset via `crossterm`
  - Files: `crates/cli/src/term_reset.rs`, used by `attach.rs`
  - Use `LeaveAlternateScreen`, `cursor::Show`, `style::ResetColor`, `terminal::EnableLineWrap`
  - Emit `CSI ? 2004 l` only if required
- [ ] Adjust sanitize to preserve CR and be less aggressive
  - Files: `crates/core/src/adapters/pty/sanitize.rs`
  - Stop converting isolated `\r` to `\n`; retain complete ANSI sequences; drop only incomplete fragments
- [ ] Expand tests to cover sanitize changes
  - Files: `crates/core/tests/pty.rs`
  - Adjust expectations to preserve CR; ensure replay alignment and ANSI preservation


## Notes

- We intentionally avoid kitty keyboard protocol parsing and any terminal state tracking to reduce complexity and ensure cross-terminal behavior.
- Cursor style is not forced; terminals choose their default styles.
- If needed later, we may add minimal detectors (alt-screen already exists) to choose reset intensity, but the current approach remains stateless.


---

## Addendum: Full Crokey-Driven Input With Structured RPC

The team agreed to go all-in on crokey and remove raw byte handling from the CLI.
Attach is TTY-only.
Input is represented as structured crokey `KeyCombination`s and sent to the daemon.
A minimal, standardized ANSI/VT100 encoding is applied at the daemon only to feed the PTY.

### Decisions

- Send structured `KeyCombination` events over RPC instead of raw bytes.
- Require a TTY for `attach`; drop pipe-based compatibility.
- Support any crokey-parsed combinations with simple, predictable behavior.
- Avoid raw ANSI handling in the CLI entirely.

### Implementation Overview

- Adopt crokey for all input parsing and representation in the CLI.
- Replace the byte-oriented stdin handler with a crokey-based event reader and combiner.
- Introduce a new `pty.input_events` RPC method carrying sequences of `KeyCombination`s.
- Centralize minimal ANSI/VT100 encoding in the daemon to write to the PTY.
- Enforce TTY-only operation for `attach`.
- Update tests to run under a pseudo-terminal harness.
- Keep configuration unchanged; parse detach keys via crokey and display them using crokey’s formatter.

### Steps

1. CLI: Crokey Event Reader And Sequence Matching
   - Files: `crates/cli/src/event_reader.rs`, `crates/cli/src/stdin_handler.rs` (deprecate), `crates/cli/src/util/detach_keys.rs`
   - Add `event_reader.rs` to read `crossterm::event::Event::Key` and use `crokey::Combiner` to produce `KeyCombination`.
   - Define `EventMsg` variants for combinations and matched binding IDs.
   - Update `detach_keys::parse_detach_keys(s: &str) -> Vec<KeyCombination>` using `crokey::parse`.
   - Implement simple sequence matcher for bindings (`Vec<KeyCombination>`), supporting multi-combo sequences like `ctrl-p,ctrl-q`.
   - Deprecate `stdin_handler.rs` and remove byte-based matching once tests are migrated.

2. CLI: Attach Workflow Integration And TTY Enforcement
   - Files: `crates/cli/src/commands/attach.rs`
   - Enforce TTY with `std::io::stdin().is_terminal()` and `std::io::stdout().is_terminal()`, exit with friendly error otherwise.
   - Resolve detach binding from config/env via crokey, display using `KeyCombinationFormat`.
   - Spawn the crokey event reader thread, receive `EventMsg`, detect detach sequence, and set `want_detach`.
   - Batch and send structured events to the daemon via the new RPC method.

3. RPC DTOs: Structured Input Events
   - Files: `crates/core/src/rpc/mod.rs`
   - Add `KeyCodeDTO`, `ModifiersDTO`, `OneToThreeDTO`, and `KeyCombinationDTO` to represent crokey’s combinations without depending on crokey types across crates.
   - Add request struct `PtyInputEventsParams { attachment_id, events: Vec<KeyCombinationDTO> }`.

4. Daemon: Input Events Handler And Minimal Encoding
   - Files: `crates/core/src/daemon/api/pty.rs`, `crates/core/src/adapters/pty/input_encode.rs`
   - Register `pty.input_events` RPC.
   - Implement minimal encoding for common keys and modifiers (letters/digits, Enter, Backspace, Tab, arrows, Home/End, PageUp/PageDown, F1–F12).
   - Write encoded bytes to the PTY and log unknown combinations.

5. Tests: PTY Harness And Attach Tests Migration
   - Files: `crates/test-support/src/pty_harness.rs`, `crates/cli/tests/*`
   - Add a pseudo-terminal harness to run `agency attach` under a PTY and send keys.
   - Migrate existing tests to drive keys via PTY and rely on crokey detection.

6. Config And Documentation
   - Files: `crates/core/src/config/*`, `README.md`, `docs/guides/*`, `docs/plans/PLN-14-*`
   - Document crokey-supported combinations, TTY-only `attach`, and the new RPC.

7. Logging And Telemetry
   - Files: `crates/cli/src/event_reader.rs`, `crates/cli/src/commands/attach.rs`, `crates/core/src/daemon/api/pty.rs`
   - Log combining capability, resolved detach keys, sequence detection, and event/encoding stats.
