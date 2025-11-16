## ADDED Requirements
### Requirement: Track and broadcast followers per TUI (event-driven)
The daemon MUST track active followers per TUI and broadcast follower count changes to subscribers; `TuiList` MAY expose counts for diagnostics but MUST NOT be required for steady-state detection.

#### Scenario: Register follower and broadcast increment
- **WHEN** client sends `C2DControl::TuiFollow { project, tui_id }`
- **THEN** the daemon records a follower for `tui_id`, keeps the control connection open
- **AND** broadcasts `D2CControl::TuiFollowersChanged { project, tui_id, followers }` to subscribers

#### Scenario: Follower cancellation decrements and broadcasts
- **GIVEN** a follower is registered for `tui_id`
- **WHEN** the follow control connection is closed (client exits/cancels)
- **THEN** the daemon decrements the follower count for `tui_id`
- **AND** broadcasts `D2CControl::TuiFollowersChanged { project, tui_id, followers }`
- **AND** optional `TuiList` includes `followers: u32` for snapshot queries

### Requirement: Backward-compatible fallback
When follower-change broadcasts are unavailable (older daemon), TUIs MAY fall back to polling `TuiList` opportunistically; polling MUST NOT be the default when events are supported.

#### Scenario: Old daemon without follower events
- **WHEN** `TuiFollowersChanged` is not supported
- **THEN** the TUI periodically or on-action queries `TuiList` and treats missing counts as zero
