## ADDED Requirements

### Requirement: Add `--follow [<tui-id>]` to `agency attach`
The CLI MUST add an optional-valued `--follow` flag to mirror the focused task of a running TUI; it MUST accept an optional `<tui-id>` and MUST be mutually exclusive with other attach modes.
#### Scenario: Follow single open TUI without ID
- Given exactly one open TUI in the current project
- When the user runs `agency attach --follow`
- Then attach follows that TUI's focused task
- And initially attaches to the focused task's session if running
- And switches to newly focused tasks as they change

#### Scenario: Multiple TUIs open without ID
- Given more than one open TUI in the current project
- When the user runs `agency attach --follow`
- Then it fails with the message:
  - `More than one TUI open. Please provide a TUI ID --follow <tui-id>. You can find the TUI ID in the top right corner.`

#### Scenario: Explicit TUI ID provided
- Given one or more open TUIs in the current project
- When the user runs `agency attach --follow <id>`
- Then attach follows the TUI with ID `<id>`
- And errors if the specified TUI is not open

#### Scenario: Coexistence with existing attach options
- `agency attach <slug|id>` continues to work unchanged
- `agency attach --session <sid>` continues to work unchanged
- `agency attach --follow [<id>]` is mutually exclusive with the positional `task` and `--session`

### Requirement: Spawn/replace attach process on focus change
When following, the CLI MUST manage a single foreground child process that reflects the current focus: an attach child for running sessions or a fallback overlay child when no session exists. On focus change, the CLI MUST terminate the prior child and spawn the appropriate new child.
#### Scenario: Focus moves to a task with a running session
- Given attach is following a TUI
- When the user changes focus in the TUI to a task with a running session
- Then the CLI terminates any existing child (attach or overlay)
- And spawns a new `tmux attach-session -t <session>` child that takes over the terminal

### Requirement: Cancel on TUI exit
If the followed TUI exits or becomes unreachable per daemon liveness checks, the CLI MUST cancel follow mode and log an error.
#### Scenario: Follow cancels if TUI closes
- Given attach is following a TUI
- When the followed TUI is closed or becomes unreachable per daemon
- Then `agency attach --follow` cancels and logs an error

### Requirement: Visible TUI ID in the UI
The TUI MUST display the TUI ID in the Tasks frame title, right-aligned. The text `TUI ID:` MUST use the default color, and only the numeric `<id>` MUST be highlighted in cyan.
#### Scenario: Display TUI ID in Tasks frame title
- Given the TUI is running
- Then the Tasks frame title shows `TUI ID: <id>` (with `<id>` cyan)
- And IDs start at 1 and increment per project instance

### Requirement: Resolve single/multiple open TUIs
The CLI MUST use the daemon-maintained registry to determine open TUIs and their focus; it MUST query via `TuiList` and act accordingly.
#### Scenario: Determine open TUI instances
- The daemon tracks open TUIs for the project and their focus
- `agency attach --follow` queries `TuiList` to resolve the single/none/multiple state
- Auto-pick only when exactly one TUI is open; otherwise require an explicit ID

### Requirement: Fallback inline overlay when no session
When the focused task has no running session, the CLI MUST present a minimal inline overlay (no raw mode, no frame) that prompts to start, and MUST switch to an attach child once the session exists.
#### Scenario: No session for focused task
- Given attach is following a TUI
- And the focused task has no running session
- Then print a centered message without a frame:
  - `No session for Task <slug> (ID: <id>). Press 's' to start, C-c to cancel.`
- Typing `s` starts the session
- The follower then spawns an attach child for the new session when the daemon snapshot reflects it
- The inline overlay re-renders for new focus or clears when a session exists

### Requirement: Cancel on user detach
If the user manually detaches the tmux attach child, the CLI MUST cancel follow and exit.
#### Scenario: Follow cancels on manual detach
- Given attach is following a TUI and attached to a session
- When the user detaches from tmux (e.g., prefix + d)
- Then `agency attach --follow` cancels and returns to the shell
