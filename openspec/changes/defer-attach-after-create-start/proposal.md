# Change: Defer attach when TUI is followed

## Why
When a TUI is being followed with `agency attach --follow`, immediately attaching in the TUI terminal after creating or starting a task is not intuitive and breaks the user's flow. In this scenario, the follower should take over attachment while the TUI remains in control.

## What Changes
- Do not change general CLI defaults or behavior
- Only when a TUI instance is being followed via `agency attach --follow`:
  - TUI create/start flows MUST not auto-attach in the TUI terminal
  - The TUI MUST remain in control
  - The follower continues to attach to the focused task as it changes
  - Outside of this follow context, keep current behavior (including start-and-attach flows)

## Impact
- Affected specs: tui-flow-hints, attach-defer-when-following
- Affected code: `crates/agency/src/tui/mod.rs`, `crates/agency/src/commands/new.rs`, `crates/agency/src/commands/start.rs` (only to the extent they are triggered by TUI flows)
