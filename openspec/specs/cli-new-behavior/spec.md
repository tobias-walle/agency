# cli-new-behavior Specification

## Purpose
Defines the behavior of the `agency new` CLI command, including its default start-and-attach flow, draft creation, no-attach semantics, and description handling without invoking an editor.

## Requirements
### Requirement: CLI `agency new` default starts and attaches without editor
The CLI MUST create a new task, start its session, and attach to it when the user runs `agency new <slug>` without `--draft` or `--no-attach`, and it MUST NOT open the editor in this default flow.

#### Scenario: `agency new <slug>` starts and attaches
- Given the user is in a git repository initialized for Agency
- And the user has configured at least one agent in `.agency/agency.toml`
- When the user runs `agency new alpha-task`
- Then Agency creates a new task with slug derived from `alpha-task`
- And writes the task markdown file with valid front matter and an empty or explicitly provided description
- And starts a session for the new task
- And attaches the user to the running session
- And the task is logged as created in the CLI output

### Requirement: CLI `agency new --draft` creates a draft without start or attach
The CLI MUST create a new task as a draft and MUST NOT start or attach to a session when the user passes `--draft` to `agency new`, while preserving the existing editor-based drafting flow in interactive mode when no description is provided.

#### Scenario: `agency new --draft <slug>` opens editor in interactive mode without description
- Given the user is in a git repository initialized for Agency
- And the user has configured at least one agent in `.agency/agency.toml`
- And the CLI is running in an interactive TTY
- And the user does not provide a positional description or `--description` flag
- When the user runs `agency new --draft zeta-task`
- Then Agency opens the configured editor to let the user author the initial description
- And only writes the task file if the user saves a non-empty description
- And does not start or attach to any session

### Requirement: CLI `agency new --no-attach` starts without attaching
The CLI MUST create a new task and start its session, but MUST NOT attach, when the user passes `--no-attach` to `agency new`.

#### Scenario: `agency new --no-attach <slug>` starts without attaching
- Given the user is in a git repository initialized for Agency
- And the user has configured at least one agent in `.agency/agency.toml`
- When the user runs `agency new --no-attach beta-task`
- Then Agency creates a new task with slug derived from `beta-task`
- And writes the task markdown file with valid front matter and an empty or explicitly provided description
- And starts a session for the new task
- And DOES NOT attach the user to the running session
- And the user can attach later with `agency attach`

### Requirement: CLI `agency new` description flags bypass editor
The CLI MUST accept both a positional description and a `--description` flag for `agency new`, use them to populate the task body without opening an editor, and still respect `--draft` and `--no-attach` semantics.

#### Scenario: `agency new` with `--description` creates without editor
- Given the user is in a git repository initialized for Agency
- And the user has configured at least one agent in `.agency/agency.toml`
- When the user runs `agency new --description "Automated body" gamma-task`
- Then Agency creates a new task with slug derived from `gamma-task`
- And writes the task markdown file with a body containing "Automated body"
- And MUST NOT open the editor
- And, because `--draft` is not set, Agency starts a session for the new task
- And attaches the user to the running session

#### Scenario: `agency new --draft` with description drafts without start or attach
- Given the user is in a git repository initialized for Agency
- And the user has configured at least one agent in `.agency/agency.toml`
- When the user runs `agency new --draft --description "Draft body" delta-task`
- Then Agency creates a new task with slug derived from `delta-task`
- And writes the task markdown file with a body containing "Draft body"
- And MUST NOT open the editor
- And MUST NOT start any session for that task
- And MUST NOT attach to any session
