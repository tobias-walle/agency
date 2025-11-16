# Design: Attach follow TUI focus

## Overview
Implement `--follow [<tui-id>]` for `agency attach` using daemon-managed TUI registration and focus events. The daemon becomes the single source of truth for open TUIs and their focused task, with a lightweight polling to verify liveness.

## Assumptions
- The follower runs outside of tmux. The implementation only spawns `tmux attach-session` children (or a fallback overlay) and never targets existing tmux clients or uses `switch-client`.

## Components
### Components
- Daemon protocol extensions:
  - `C2DControl::TuiRegister { project, pid }` → `D2CControl::TuiRegistered { tui_id }`
  - `C2DControl::TuiUnregister { project, pid }` (best-effort; daemon also cleans up stale)
  - `C2DControl::TuiFocusTaskChange { project, tui_id, task_id }`
- `C2DControl::TuiFollow { project, tui_id }` → `D2CControl::TuiFollowSucceeded { tui_id }` or `D2CControl::TuiFollowFailed { message }`
- `D2CControl::TuiFocusTaskChanged { project, tui_id, task_id }`
  - `C2DControl::TuiList { project }` → `D2CControl::TuiList { items: Vec<{ tui_id, pid, focused_task_id: Option<u32> }> }`
- Daemon state:
  - Per-project map: `tui_id -> { pid, last_seen, focused_task_id: Option<u32> }`
  - ID assignment: lowest free positive integer; IDs per-project start at 1
  - Liveness checker: every 10s, verify PID is alive; purge entries whose processes exited; broadcast changes as needed
- TUI UI changes:
  - Display `TUI Id: <id>` in the Tasks frame title on the right side; `<id>` in cyan
  - On selection change, send `TuiFocusTaskChange` with the selected task id
- Attach follow flow:
  - Resolve target TUI ID (explicit or only-open auto-pick via `TuiList`; else error with the provided message)
  - Send `TuiFollow`; handle `TuiFollowSucceeded/Failed`. On success, subscribe to daemon events and react to `TuiFocusTaskChanged` (daemon also emits the current focus immediately after success)
  - On event: if focused task has a running session → spawn a child `tmux attach-session -t <session>` (kill any previous child)
  - If no session → spawn a minimal overlay app child that shows a fullscreen message; pressing `s` starts the session; on success, terminate overlay child and start an attach child
  - If TUI disappears (daemon indicates missing or liveness check drops entry) → cancel follow and log an error

### Process orchestration
- The follower CLI maintains at most one child process at a time:
  - Attach child: runs `tmux attach-session -t <session>` and inherits the terminal; when the user detaches (prefix+d), the child exits and the follower remains running, awaiting the next focus event.
  - Overlay child: small ratatui app that informs "No session for Task <slug> (ID: <id>). Press s to start.". Key `s` triggers the start helper. On focus change, the follower terminates the overlay child immediately.
- On `TuiFocusTaskChanged`, the follower gracefully terminates any existing child (send SIGTERM, fall back to SIGKILL after a short timeout) and spawns the new appropriate child.
- This design removes any dependency on `switch-client` and `client_name` targeting, so follow works even when no tmux client is active at the moment.

## Data structures
Daemon-tracked focus: `focused_task_id: Option<u32>` per registered TUI.

## Error handling
- If multiple TUIs are open and `--follow` without id: print exact guidance string.
- If focused task is `None` or task not found: spawn the overlay child.
- If TUI disappears or PID dies: cancel follow and log an error.
- Child spawn/termination errors are surfaced with concise messages; follower attempts a best-effort cleanup before exiting.

## Future upgrade path
Already covered in Components; this is the chosen path for this change.
