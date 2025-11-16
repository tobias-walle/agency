## ADDED Requirements
### Requirement: Defer auto-attach in TUI when followed
If a `agency attach --follow` process is actively following the current TUI instance for this project, TUI-initiated start-and-attach flows MUST skip attaching in the TUI terminal.

#### Scenario: TUI start-and-attach while followed
- **GIVEN** a follower is active and subscribed to the current TUI
- **WHEN** the user triggers a start-and-attach action from the TUI (e.g., New+Start)
- **THEN** the session is started
- **AND** the TUI does not attach
- **AND** the follower attaches to the focused task's session

### Requirement: Event-driven follower detection with fallback
The TUI MUST subscribe to follower-change events and maintain an in-memory `followers` count for its own `tui_id`. It MAY fall back to `TuiList` only when events are not available.

#### Scenario: Event-driven updates
- **GIVEN** the TUI subscribed to daemon events
- **WHEN** it receives `D2CControl::TuiFollowersChanged { tui_id, followers }` for its id
- **THEN** it updates local follower state immediately and uses it to decide whether to defer attach

#### Scenario: Fallback when events unsupported
- **GIVEN** follower-change events are not supported by the daemon
- **WHEN** the TUI needs follower state
- **THEN** it queries `TuiList` as a fallback and proceeds
