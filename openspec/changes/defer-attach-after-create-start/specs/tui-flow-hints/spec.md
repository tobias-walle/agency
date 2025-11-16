## ADDED Requirements
### Requirement: TUI defers attach when followed
When a task is created or started from the TUI and the current TUI instance is being followed via `agency attach --follow`, the TUI MUST remain in control without auto-attaching. The task list selection SHOULD move to the created/started task.

#### Scenario: Create without auto-attach while followed
- **GIVEN** the current TUI is being followed
- **WHEN** the user creates a task from the TUI
- **THEN** the TUI remains active and focused
- **AND** the new task is selected in the list

#### Scenario: Start without auto-attach while followed
- **GIVEN** the current TUI is being followed
- **WHEN** the user starts a task from the TUI
- **THEN** the TUI remains active and focused
- **AND** the started task remains selected

### Requirement: No additional prompts added
The TUI MUST NOT add additional prompts, banners, or transient popups for this behavior change.

#### Scenario: No extra prompt shown
- **GIVEN** the current TUI is being followed
- **WHEN** the user starts a task from the TUI
- **THEN** no additional UI prompt is shown

### Requirement: Follow-mode friendly behavior
Starting a session from the TUI MUST NOT attach in the TUI terminal when followed so that a follower (`agency attach --follow`) can attach in its own terminal.

#### Scenario: TUI start while follower runs
- **GIVEN** a follower is active for the current TUI
- **WHEN** the user starts a task from the TUI
- **THEN** the TUI does not attach
- **AND** the follower attaches to the session instead

### Requirement: Preserve current behavior when not followed
When the TUI is not being followed, existing behavior MUST remain: start-and-attach flows continue to attach from the TUI as they do today.

#### Scenario: TUI start-and-attach without follower
- **GIVEN** the current TUI is not being followed
- **WHEN** the user starts a task using a start-and-attach flow
- **THEN** the TUI attaches as it does today
