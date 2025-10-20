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

- [ ] Tests first: leverage existing CLI/core tests; add minimal updates
  - Files: `crates/cli/tests/attach_e2e.rs`, `attach_fast_input.rs`
  - Verify second attach with fake shell shows no indentation drift
- [ ] Integrate `crokey` for detach parsing
  - Files: `crates/cli/src/util/detach_keys.rs`, `crates/cli/src/commands/attach.rs`
  - Replace custom parsing with `crokey::parse` for env/config strings
  - Map `KeyCombination` to legacy byte(s) for binding setup
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
