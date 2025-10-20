## Crokey best practices distilled

- Use Crokey’s `Combiner` to transform `crossterm::event::KeyEvent` into normalized `KeyCombination` values across terminals.
- Call `enable_combining()` once per attach session to activate Kitty keyboard enhancements when available, and fall back gracefully on ANSI terminals.
- Parse config strings via `crokey::parse()` into `Vec<KeyCombination>` for detach sequences.
- Use the `key!` macro for compile-time literals and pattern matching in internal code.
- Maintain a small suffix buffer of recent `KeyCombination`s to match multi-step sequences like `ctrl-p,ctrl-q`.
- Keep raw mode enabled for the session and enable bracketed paste.
- Treat paste as a distinct event; forward directly as bytes, not through the Combiner.
- Convert Crokey `KeyCombination` to app DTOs only at the RPC boundary and encode to bytes only on the daemon side.
- Avoid `^S/^Q` as control keys due to possible flow-control swallowing.
- Ensure proper teardown of keyboard enhancement flags on detach/end of session.

## Plan: Fully utilize Crokey and fix detach regression

### Implementation approach

- Centralize input normalization with Crokey’s `Combiner`.
- Match detach on normalized `KeyCombination`s, not raw bytes.
- Convert combos to existing RPC DTOs at the boundary, splitting multi-key combos if needed.
- Keep raw mode and bracketed paste enabled for the duration of attach.
- Simplify code by removing legacy byte-oriented input and ad‑hoc key handling.

### Steps

1. Input normalization with Crokey Combiner
   Files: `crates/cli/src/event_reader.rs`, `crates/cli/src/commands/attach.rs`
   - Add `Combiner` initialization per attach session and call `enable_combining()`.
   - In `event_reader`, transform `KeyEvent` to `KeyCombination` via `combiner.transform()` and send `EventMsg::Combo` using Crokey types internally.
   - Handle `Event::Paste(String)` distinctly and forward as a paste payload message, bypassing the Combiner and detach matcher.
   - Keep `terminal::enable_raw_mode()` active for the session and `DisableBracketedPaste` on teardown.

2. Detach sequence parsing and matching
   Files: `crates/cli/src/util/detach_keys.rs`, `crates/cli/src/commands/attach.rs`
   - Parse config env `AGENCY_DETACH_KEYS` using `crokey::parse()` into `Vec<KeyCombination>` (store Crokey types for matching).
   - Maintain a ring buffer of recent `KeyCombination`s and match suffix equality against the parsed detach sequence.
   - On match, trigger detach and do not forward those combos to RPC.

3. DTO conversion at RPC boundary
   Files: `crates/cli/src/rpc/client.rs`, `crates/cli/src/commands/attach.rs`
   - Convert Crokey `KeyCombination` to `agency_core::rpc::KeyCombinationDTO` only when building `pty.input_events` payloads.
   - If a Crokey combo contains multiple non-modifier keys (Kitty chord), split into multiple single-key DTO events sharing modifiers to fit current DTO shape.
   - Keep current daemon-side encoder `crates/core/src/adapters/pty/input_encode.rs` to map DTOs to bytes.

4. Bracketed paste support
   Files: `crates/cli/src/commands/attach.rs`, `crates/cli/src/event_reader.rs`
   - Enable bracketed paste on attach and disable on detach.
   - Forward paste payload as a dedicated RPC call or encode as bytes directly (choose one; recommended: a dedicated RPC `pty.input_paste` to avoid splitting large strings into per‑key events).
   - Update logs to differentiate paste vs. key batches.

5. Remove legacy byte-oriented input paths and simplify
   Files: `crates/cli/src/stdin_handler.rs`, `crates/cli/src/lib.rs`
   - Deprecate and remove references to `stdin_handler` in attach.
   - Keep the module only if still used elsewhere; otherwise, plan removal in a follow-up PR to reduce dead code.

6. Logging and diagnostics
   Files: `crates/cli/src/commands/attach.rs`
   - Log Crokey format strings for resolved detach keys (e.g. `Ctrl-q`) using `KeyCombinationFormat`.
   - Add `cli_detach_requested` when the matcher triggers to make debugging straightforward.
   - Log whether Kitty combining is enabled.

7. Tests and CI
   Files: `crates/cli/tests/*.rs`, `crates/core/tests/*.rs`
   - Add unit tests for `detach_keys` parsing via `crokey::parse()` (case, spaces, punctuation).
   - Add tests for suffix matching on Crokey `KeyCombination`s, including `ctrl-p,ctrl-q`.
   - Add a test for conversion of Crokey combos to DTOs, including splitting multi-key combos.
   - Add tests for paste handling as distinct events.
   - Keep attach e2e tests focused on behavior, not raw stdin bytes, to avoid TTY conflicts.

8. Documentation updates
   Files: `docs/guides/RUST_BEST_PRACTICES.md` (if needed), `docs/plans/PLN-14-stateless-reset-crokey-detach-sanitize-replay.md`, `README.md`
   - Document Crokey’s role, bracketed paste behavior, and constraints around detach keys.
   - Update examples to show `AGENCY_DETACH_KEYS` as Crokey-parsable strings.

### Key APIs/classes to add or adjust (high level)

- `event_reader::spawn_event_reader`
  - Use Crokey `Combiner` to produce `KeyCombination` for `Event::Key`.
  - Emit `EventMsg::Combo(KeyCombination)` and `EventMsg::Paste(String)`.

- `util::detach_keys::parse_detach_keys()`
  - Return `Vec<crokey::KeyCombination>` instead of DTOs.

- `commands::attach::attach_interactive()`
  - Initialize raw mode and bracketed paste.
  - Maintain ring buffer of `KeyCombination`.
  - Convert combos to DTOs only for RPC.
  - Split multi-key combos as needed.

- RPC client session functions
  - Keep `pty.input_events` as today, using `Vec<KeyCombinationDTO>`.
  - Optionally add `pty.input_paste` for bulk paste payloads.

- Daemon PTY encoder
  - No change required for single-key DTOs.
  - If `pty.input_paste` is added, map directly to bytes and write to PTY.
