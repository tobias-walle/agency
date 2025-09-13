# PLAN-9: PTY detach emits terminal reset footer (restore cursor color and modes)

## Problem
Detaching from an attached TUI can leave the local terminal in a modified state (e.g., changed cursor color, hidden cursor, bracketed paste, mouse modes, alt-screen). 
Currently the CLI exits raw mode and prints a message, but does not proactively restore terminal state, so the cursor color and other settings may remain changed after detach.

## Goal
Emit a small, safe, idempotent “terminal reset footer” to stdout when leaving `agency attach`, to restore common terminal modes and dynamic colors (including cursor color) to the terminal’s defaults.

- Restore cursor color to default (addresses the reported issue) using `OSC 112`.
- Leave alternate screen, show the cursor, reset SGR, disable bracketed paste and common mouse modes.
- Keep behavior safe across terminals: use broadly supported sequences; unknown ones should be ignored.
- Avoid polluting redirected output by guarding emission with a TTY check.

## Non-Goals
- Do not attempt a full terminal hard reset (`ESC c`) because that is disruptive.
- Do not introduce termcap/terminfo dependencies.
- Do not alter daemon/PTY behavior — this change is CLI‑side hygiene only.

## References
- XTerm Control Sequences: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html
  - Soft reset: `CSI ! p` (DECSTR)
  - Alt-screen leave: `CSI ? 1049 l` (also `?1047l`, `?47l`)
  - Show cursor: `CSI ? 25 h`
  - Reset SGR: `CSI 0 m`
  - Bracketed paste off: `CSI ? 2004 l`
  - Mouse modes off: `?1000`, `?1002`, `?1003`, `?1005`, `?1006`, `?1015`, `?1016`
  - Dynamic color resets: `OSC 110/111/112` with ST terminator (ESC `\\`)

## Approach
Implement a terminal reset footer emitted by the CLI on exit from interactive attach, both on explicit detach and on EOF, guarded by a TTY check on stdout.

### Sequences to emit (order is safe and idempotent)
- `CSI ! p` (DECSTR) soft reset of many modes
- `CSI ? 1049 l`, `CSI ? 1047 l`, `CSI ? 47 l` leave alt-screen
- `CSI ? 25 h` show cursor
- `CSI 0 m` reset SGR
- `CSI ? 2004 l` disable bracketed paste
- `CSI ? 1000 l`, `CSI ? 1002 l`, `CSI ? 1003 l`, `CSI ? 1005 l`, `CSI ? 1006 l`, `CSI ? 1015 l`, `CSI ? 1016 l` disable common mouse modes
- `OSC 110 ST` reset default foreground color
- `OSC 111 ST` reset default background color
- `OSC 112 ST` reset cursor color (fixes the reported issue)
- Optional: `CSI 0 q` restore default cursor style (harmless if unsupported)

Use ST as the OSC terminator (ESC `\\`), which is standard and avoids edge cases; BEL is also accepted but not preferred here.

## Implementation Plan

### 1) New module with documented constants
File: `crates/cli/src/term_reset.rs`

- Provide well‑named, documented constants (`&[u8]`) for readability and maintainability. 
- Include inline comments referencing ctlseqs where useful, and explain safety/idempotence.
- Provide a helper:
  - `pub fn write_reset_footer<W: std::io::Write>(w: &mut W) -> std::io::Result<()>` which writes the robust set above in order and flushes.

Suggested constants:
- Introducers/terminators: `ESC`, `CSI`, `OSC`, `ST`, `BEL` (for documentation/reference; not necessarily used directly)
- Core reset: `DECSTR`, `LEAVE_ALT_1049`, `LEAVE_ALT_1047`, `LEAVE_ALT_47`, `SHOW_CURSOR`, `SGR_RESET`
- Input modes off: `BRACKETED_PASTE_OFF`, mouse mode offs for `?1000`, `?1002`, `?1003`, `?1005`, `?1006`, `?1015`, `?1016`
- Dynamic color resets: `OSC_RESET_FG` (110 ST), `OSC_RESET_BG` (111 ST), `OSC_RESET_CURSOR` (112 ST)
- Optional: `CURSOR_STYLE_DEFAULT` (`CSI 0 q`)

### 2) Wire into CLI attach cleanup
File: `crates/cli/src/lib.rs`

- In `attach_interactive` after sending `pty_detach`, if stdout is a TTY, call `term_reset::write_reset_footer(&mut stdout)`.
- Also ensure the EOF path (when the PTY exits) triggers the same footer before leaving the loop.
- Use a TTY guard to avoid emitting control sequences into pipelines:
  - Rust std: `use std::io::IsTerminal; if std::io::stdout().is_terminal() { ... }`

### 3) Tests (TDD)
File: `crates/cli/tests/attach_resets_terminal.rs`

- Write a failing E2E test that:
  - Initializes a temp repo and project, starts a task, and runs `agency attach`.
  - Sends `printf '\x1b]12;red\x07'` (OSC 12 cursor color) to mutate cursor color, then sends the configured detach sequence (e.g., Ctrl‑Q).
  - Captures stdout and asserts that at detach the CLI emitted:
    - `\x1b]112\x1b\\` (OSC 112 ST) – reset cursor color
    - `\x1b[!p` (DECSTR)
    - `\x1b[?1049l` (leave alt‑screen)
- Keep assertions readable and focused; do not overfit to ordering beyond essentials if not required.

### 4) Docs (optional)
- Add a short note to `docs/plans/PLN-2-phase-10-pty-completion.md` or a new brief entry that the CLI now emits a reset footer on detach and that cursor color is restored via `OSC 112`.

## Acceptance Criteria
- On explicit detach and on EOF, when stdout is a TTY, the CLI writes the reset footer to stdout.
- The E2E test detects at least `OSC 112 ST`, `DECSTR`, and `?1049l` in stdout on detach.
- When stdout is not a TTY, no reset sequences are written (add a negative test if desired).
- Implementation uses named constants with comments in `crates/cli/src/term_reset.rs` and a helper function for emission.
- Changes are minimal, isolated to the CLI; no unrelated code is altered.

## Files
- New: `crates/cli/src/term_reset.rs`
- Update: `crates/cli/src/lib.rs` (in `attach_interactive`)
- New test: `crates/cli/tests/attach_resets_terminal.rs`
- Optional doc note: `docs/plans/PLN-2-phase-10-pty-completion.md`

## Rollout & Risks
- Sequences are idempotent and commonly supported; unknown ones are ignored.
- Guarded by TTY detection to avoid polluting piped output.
- Avoid `ESC c` (hard reset) to reduce disruption.

## Execution Notes
- Follow project testing and formatting: `just test`, `just fmt`.
- Keep commit messages conventional and scoped to this feature.
