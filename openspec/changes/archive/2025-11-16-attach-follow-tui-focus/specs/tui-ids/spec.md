## ADDED Requirements

### Requirement: Assign per-project TUI IDs starting at 1
Each TUI instance MUST register with the daemon and MUST receive the lowest free positive integer ID scoped to the project, starting at 1.
#### Scenario: First TUI start
- Given no open TUI in the project
- When starting the TUI
- Then it registers with the daemon and receives TUI ID `1`

#### Scenario: Concurrent TUI start
- Given one or more open TUIs
- When starting another TUI
- Then the daemon assigns the lowest free positive integer ID

#### Scenario: Cleanup stale entries
- The daemon periodically (about every 10 seconds) verifies registered TUI PIDs are alive and removes stale entries

### Requirement: Show ID in UI
The TUI MUST render its assigned ID in the Tasks frame title on the right side with the numeric ID in cyan.
#### Scenario: Tasks frame title shows ID at top-right
- The Tasks table title includes `TUI Id: <id>` aligned to the right side
- The `<id>` is rendered in cyan
