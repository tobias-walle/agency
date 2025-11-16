# Design: TUI `start` and `new+start` run with `--no-attach`

## Architecture Overview

- Files impacted:
  - `crates/agency/src/tui/mod.rs` – key handlers for Start (`s`) and New overlays (`n`/`N`).
- Symbols impacted:
  - `Mode::List` key handler for `KeyCode::Char('s')` – switch to `start::run_with_attach(..., false)`.
  - `Mode::InputSlug { start_and_attach: true }` enter handler – after `new::run(...)`, switch to `start::run_with_attach(..., false)`.

## Flow Changes

- Current:
  - `s` → `start::run(ctx, ident)` attaches.
  - `N` → `new::run(...)` then `start::run(ctx, id)` attaches.
- Updated:
  - `s` → `start::run_with_attach(ctx, ident, false)` no-attach.
  - `N` → `new::run(...)` then `start::run_with_attach(ctx, id, false)` no-attach.

## Trade-offs

- Readability: Small, targeted calls; no broad refactors.
- Maintainability: Uses existing `run_with_attach` API; no new flags.
- UX: Prioritizes keeping TUI active; attach left to explicit user action.

## Alternatives Considered

- Add a TUI-level config toggle; deferred for simplicity.
- Spawn attach in a separate terminal; rejected due to complexity and platform variance.

