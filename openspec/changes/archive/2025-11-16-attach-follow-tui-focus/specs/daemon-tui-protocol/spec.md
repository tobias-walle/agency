## ADDED Requirements

### Requirement: TUI lifecycle managed by daemon
The daemon MUST assign and track per-project TUI IDs, handling registration, deregistration, and cleanup of stale entries.
#### Scenario: Register TUI
- Client sends `C2DControl::TuiRegister { project, pid }`
- Daemon assigns the lowest free positive integer ID for the project
- Daemon replies with `D2CControl::TuiRegistered { tui_id }`

#### Scenario: Unregister TUI
- Client sends `C2DControl::TuiUnregister { project, pid }`
- Daemon removes the registration and frees the ID (best-effort)
- Daemon’s periodic liveness check also removes stale entries

### Requirement: TUI focus events
The daemon MUST accept focus-change updates from TUIs and MUST broadcast focus-change events to subscribers.
#### Scenario: Focus changes in TUI
- Client sends `C2DControl::TuiFocusTaskChange { project, tui_id, task_id }`
- Daemon updates internal focus state and broadcasts `D2CControl::TuiFocusTaskChanged { project, tui_id, task_id }` to subscribers

### Requirement: Follow handshake for attach
The attach client MUST initiate following a specific TUI via a handshake and MUST receive immediate and subsequent focus-change events.
#### Scenario: Start following a TUI
- Client sends `C2DControl::TuiFollow { project, tui_id }`
- Daemon replies with either `D2CControl::TuiFollowSucceeded { tui_id }` or `D2CControl::TuiFollowFailed { message }`
- Once following, the client receives `D2CControl::TuiFocusTaskChanged` events for that TUI
 - After `TuiFollowSucceeded`, the daemon immediately sends the current focus (if any) for the TUI as a `TuiFocusTaskChanged` event to initialize the client state

### Requirement: Daemon liveness checks
The daemon MUST periodically verify registered TUIs’ PIDs and MUST remove dead entries to keep state accurate.
#### Scenario: Periodic verification
- The daemon verifies registered TUI PIDs are alive approximately every 10 seconds
- When a TUI process is no longer alive, the daemon removes the registration and may broadcast a state update


### Requirement: Query open TUIs
Clients MUST be able to list open TUIs and their current focus for a project using a dedicated request.
#### Scenario: List open TUIs in a project
- Client sends `C2DControl::TuiList { project }`
- Daemon replies with `D2CControl::TuiList { items: Vec<{ tui_id, pid, focused_task_id: Option<u32> }> }`
- Used by `agency attach --follow` to auto-select when exactly one TUI is open
