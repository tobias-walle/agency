## ADDED Requirements

### Requirement: Minimal overlay app for missing session
The follow mode MUST provide a small, independent ratatui overlay when no session exists for the focused task, with a clear prompt to start.
#### Scenario: Focused task has no session
- The follow attach mode renders a small ratatui app (independent of the main TUI) that fills the terminal
- The app shows exactly: `No session for Task <slug> (ID: <id>). Press s to start.`
- The app exits when:
  - The user presses `s` (after starting the session)
  - Or the focused task changes (as reported by the daemon)

#### Scenario: Starting a session from overlay
- Pressing `s` invokes the existing session start helpers to create a tmux session for the task
- On success, the overlay app exits and the attach process switches to the new session

### Requirement: Real-time updates from daemon
The overlay MUST react to daemon focus-change events for the followed TUI, updating or exiting immediately when appropriate.
#### Scenario: Immediate overlay updates
- The overlay subscribes to daemon events (e.g., `TuiFocusTaskChanged`) for the followed TUI
- When focus changes, the overlay immediately reflects the new task or exits if a session is running and attach switches
