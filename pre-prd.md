# Orchestra

The orchestra tool should help orchestrate parallel running AI CLI Agents.

## Features

- Manage and create git worktrees (Based on the current branch or a specific branch)
- Run any AI CLI Agent in the background, with the possibility of attach into the process
- Easily cd into a specific worktree
- configurable `setup` script (stored in a local config folder) that runs in every new worktree to setup stuff. Context is provided via env vars.
- Server/Client Architecture with multiple interfaces
  - CLI Interface, e.g.
    - `orchestra new [task]` (e.g. `[task]` is a short slug, e.g. "add-docs") - Create a new task with a specific name and open $EDITOR to change the description in a markdown document
    - `orchestra edit [task]` - Edit a task
    - `orchestra start [task]` - Start the task in an AI agent in the background
    - `orchestra stop [task]` - Stop the AI agent
    - `orchestra attach [task]` - Attach to the running agent of the task (If it is running)
    - `orchestra complete [task]` - Mark a task as completed
    - `orchestra status` - See an overview of the tasks with the status `running`, `idle`, `draft`, `completed`
    - `orchestra merge [task]` - Merge a task back to current branch
  - MCP Server (Which allows the other agents to interact with the task runner, e.g. create and start tasks or mark them as completed)
  - Low Prio: Live CLI Tui Interface to watch the tasks
- The overhead of managing all these interfaces should be minimal
- The server should use JSON RPC if feasible
- Initially supported Tools (In the best case the tool is generic enough that it supports new tools with minimal setup):
  - Opencode (First prio)
  - Claude Code (Second prio)
  - Other Tools (In the future)

## Tasks

A task is the basic primitive of orchestra. A task is a specific work unit and has a slug, numeric id (autoincremented) and a description

Tasks are stored locally in each project in `.orchestra/tasks/[id]:[slug].md`.
Each task has metadata associated with it, which is stored in its yaml header.

E.g. `42:fix-tests.md` with the content:

```markdown
---
base_branch: main
status: draft
---

Analyse the errors in `./src/utils/date.test.ts`, create a fix in `./src/utils/date.ts` and make sure the tests are successful.
```

The `base_branch` is the branch the task originated from and should be merged into.

Each task has a status:

- `draft` - Task was created, but not started yet. This is the default status of new tasks.
- `running` - Task that is current in work. That means an agent is actively working on it (e.g. is generating tokens).
- `idle` - Task that is current in work, but the agent is not doing anything. Often this means user input is required, so he needs to attach to the process and answer a question or prompt how to cotinue the work.
- `completed` - Task was marked as completed. Either by the agent or via the cli. Afterwards the user might attach into the session, gives feedback. This will reset the state back to running or idle.
- `reviewed` - Task was reviewed and corrected by the user. This is a manual step that is done after the task was completed. The user need to mark a task as reviewed via the cli.

The status can be updated via the cli. The user can setup hooks in their favorite tool to update the status automatically (especially idle).

## Tech Stack

- I want to use Typescript, as I am very familiar with it, with bun.
- Alternatively Rust, but the long compile times could be an issue.

## Open Questions

- Only one instance of orchestra should be running in the background. It should be able to manage multiple projects (or projects folders) at once. How can this be archived? (Focus on mac for now)
- How can the detached processes be managed? The user should be able to attach into them later and detach again. I was thinking about using tmux for this, but this might not be the best idea, as it introduces a hard dependency.
- Am I missing something in my design?
