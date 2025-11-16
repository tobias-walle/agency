## ADDED Requirements

### Requirement: Overlay for missing session (raw mode)
The follow mode MUST provide a minimal overlay when no session exists for the focused task, using raw mode and the alternate screen to render a centered prompt.
#### Scenario: Focused task has no session
- The follow attach mode renders a centered message:
  - `No session for Task <slug> (ID: <id>). Press 's' to start, C-c to cancel.`
- The overlay owns raw mode and the alternate screen during its lifetime
- The overlay disappears automatically when a session for the focused task is detected or the focus changes

#### Scenario: Starting a session from overlay
- Typing `s` invokes the existing session start helpers to create a tmux session for the task
- After the daemon reports the new session in its snapshot, follow attaches to the session automatically

#### Scenario: Cancel overlay
- Typing `C-c` cancels follow and returns control to the terminal without starting a session

### Requirement: Real-time updates from daemon
The overlay MUST react to daemon focus-change events for the followed TUI, updating or exiting immediately when appropriate.
#### Scenario: Immediate overlay updates
- The follower subscribes to daemon events (e.g., `TuiFocusTaskChanged`) for the followed TUI
- When focus changes, the overlay is re-rendered for the new task or cleared if a session is running and attach switches
