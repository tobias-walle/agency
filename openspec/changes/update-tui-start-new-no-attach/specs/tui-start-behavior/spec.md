# tui-start-behavior Change Spec

## Purpose
Align TUI actions with background session starts to maintain focus and reduce context switching.

## ADDED Requirements

### Requirement: TUI `Start` runs without attach
The TUI MUST start a task’s session without attaching when the user presses `s` in the tasks list.
#### Scenario: Start via `s`
- Given the TUI is focused on a task row
- When the user presses `s`
- Then Agency starts the session for that task without attaching
- And the TUI remains active and updates status/logs

### Requirement: TUI `New + Start` runs without attach
The TUI MUST create a task and start its session without attaching when the user confirms `New + Start`.
#### Scenario: `N` New + Start
- Given the TUI is in the slug input overlay with `start_and_attach = true`
- When the user submits the slug
- Then Agency creates the task
- And starts the session without attaching
- And the TUI remains active and updates status/logs

## MODIFIED Requirements

### Requirement: Help text clarity for background starts
The TUI MUST indicate that `Start` and `New + Start` run in the background without attach.
#### Scenario: Help bar and command logs
- Update help bar or log lines to reflect "Start (no attach)" semantics
- Preserve existing key bindings (`n`, `N`, `s`) and other behaviors

## Notes
- Explicit attach remains available via CLI or `attach --follow`.
- No changes to daemon protocol.
