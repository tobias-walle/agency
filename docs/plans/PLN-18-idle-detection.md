# PLAN: Idle Detection For PTY Sessions
Add daemon-side idle detection so tasks can surface an “Idle” status and paint it blue in clients.

## Goals
- Track PTY activity to decide when a session is idle versus actively producing output.
- Surface the `Idle` status through `SessionInfo`, task listing, and TUI rendering.
- Notify subscribers/clients whenever a session flips between running and idle.
- Reuse a centralized helper for stripping ANSI/control sequences.

## Out of scope
- Changing daemon/client transport or frame formats beyond status text.
- Making idle thresholds user-configurable.
- Persisting idle state across daemon restarts.

## Current Behavior
- `SessionStatus` distinguishes only `Running` vs `Exited`; `SessionInfo.status` mirrors this (`crates/agency/src/pty/registry.rs:43-50`, `:214-220`).
- `Session::start_read_pump` forwards raw bytes without tracking timing or visibility (`crates/agency/src/pty/session.rs:137-168`).
- Daemon loop polls only for exited children before accept (`crates/agency/src/pty/daemon.rs:102-140`) and has no concept of idle transitions.
- Task status derivation maps `SessionInfo.status` onto `TaskStatus` lacking an idle state (`crates/agency/src/utils/status.rs:6-35`), and renderers cover only Draft/Stopped/Running/Exited (`crates/agency/src/commands/ps.rs:21-38`, `crates/agency/src/tui/mod.rs:503-533`).
- Visible-length calculation maintains its own ANSI-stripping regex inside `utils::term::visible_len` (`crates/agency/src/utils/term.rs:40-74`); there’s no reusable helper for raw-output normalization.

## Solution
- Extract the existing regex into a shared `strip_ansii_control_codes` helper in `utils::term`, reusing it wherever control sequences must be ignored.
- Introduce an `IdleTracker` module that records timestamps for raw bytes and visible characters (after stripping control codes) and applies hysteresis to determine idle vs active.
- Embed the tracker in `Session`, updating it on output and input; expose a `poll_idle` method so the registry can ask for current state without holding locks while reading.
- Extend `SessionStatus` with an `Idle` variant; add a registry poller that invokes `session.poll_idle`, updates status, and reports state changes.
- In the daemon loop, run the idle poller each tick (after exit polling) and broadcast `SessionsChanged` for projects whose sessions toggled idle/active.
- Update status derivation and rendering across CLI/TUI to recognize the `"Idle"` string and color it blue.

## Architecture
- `crates/agency/src/utils/term.rs`
  - Add `pub fn strip_ansii_control_codes(s: &str) -> String` and refactor `visible_len` to use it.
- `crates/agency/src/pty/idle.rs`
  - New module containing `IdleTracker`, idle thresholds, and unit tests leveraging the shared stripping helper.
- `crates/agency/src/pty/mod.rs`
  - Re-export the new idle module.
- `crates/agency/src/pty/session.rs`
  - Store an `IdleTracker`, update it in `start_read_pump` and `write_input`, add `poll_idle`.
- `crates/agency/src/pty/registry.rs`
  - Extend `SessionStatus` with `Idle`, add `poll_idle_sessions` helper, update `list_sessions` to emit `"Idle"`.
- `crates/agency/src/pty/daemon.rs`
  - Call `poll_idle_sessions` each loop and broadcast session changes when states flip.
- `crates/agency/src/utils/status.rs`, `crates/agency/src/commands/ps.rs`, `crates/agency/src/tui/mod.rs`
  - Add `TaskStatus::Idle`, handle new color (blue), update tests.
- Tests
  - Unit tests for `IdleTracker` in `pty/idle.rs`.
  - Adjust existing CLI/TUI tests to assert Idle mappings.

## Detailed Plan
- [ ] Extract shared ANSI stripping: move the regex logic from `visible_len` into `strip_ansii_control_codes` in `utils::term.rs`, update `visible_len` to call it, and adjust any consumers (search for similar stripping code to reuse the helper).
- [ ] Implement `IdleTracker` (`pty/idle.rs`): store configurable thresholds, track last-byte and last-visible timestamps, apply hysteresis over successive polls, and cover behavior with unit tests (e.g., continuous control codes, visible bursts, UTF-8 boundaries).
- [ ] Integrate tracker into `Session`: add a field, initialize/reset in `Session::new` and `restart_shell`, update it inside the read pump before forwarding output (use `strip_ansii_control_codes` to detect visibility), reset on `write_input`, and expose `poll_idle(now)` returning `(IdleState, changed_flag)`.
- [ ] Update `SessionStatus`/registry: add `Idle` variant, create `poll_idle_sessions(now)` that iterates sessions (without long-held locks), calls `poll_idle`, toggles status, and collects `(session_id, project)` when state changes; ensure `list_sessions` produces `"Idle"` for that variant.
- [ ] Wire into daemon loop: after `poll_session_exits`, call the new registry helper, broadcast `SessionsChanged` for each affected project, and consider ordering so exit notifications still occur promptly.
- [ ] Propagate new status to clients: extend `TaskStatus` and associated mappings to include `Idle` (blue via `owo-colors` in CLI and `ratatui::style::Color::Blue` in TUI), ensure `derive_status` recognizes the `"Idle"` string, and update existing unit tests (`commands/ps.rs`, `tui/mod.rs`) plus add coverage for `status_label`.
- [ ] Finalize: run `just check` and `cargo fmt`; ensure new unit tests (idle tracker) pass and adjust snapshots/fixtures if necessary.

## Questions
1. None (fixed thresholds assumed; can revisit if configurability is desired later).
