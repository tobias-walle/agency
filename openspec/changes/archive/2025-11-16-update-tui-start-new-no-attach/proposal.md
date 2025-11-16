# Proposal: Update TUI to run `start` and `new+start` with `--no-attach`

## Summary
Change the Agency TUI so that pressing `s` (Start) and confirming "New + Start" (`N`) start sessions **without attaching**. This keeps the TUI in focus, lets users continue navigating/editing, and relies on explicit attach (or `attach --follow`) when an interactive tmux view is desired.

## Motivation
- Avoid disruptive terminal context switches from the TUI.
- Align with workflows that use `agency attach --follow` to view sessions.
- Keep TUI responsive while sessions spin up in the background.

## Scope
- TUI keyboard actions only. CLI flags and defaults remain unchanged.
- No changes to daemon protocol or `attach` behavior.

## High-Level Changes
- `s` (Start) in TUI calls `start::run_with_attach(ctx, ident, false)`.
- "New + Start" (`N`) creates the task, then calls `start::run_with_attach(ctx, id, false)`.
- Update help text if needed to clarify background start.

## Risks & Trade-offs
- Users may expect immediate tmux attach; mitigated by clear help text and existing `attach --follow`.
- Background starts require good feedback; TUI already shows status/log pane.

## Validation
- Manual: open TUI, press `s` and `N`; ensure TUI stays active and session starts.
- Automated: existing tests continue to pass; add focused tests if needed.

