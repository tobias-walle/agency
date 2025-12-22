---
name: agency
description: Use Agency CLI to run parallel AI coding tasks in isolated Git worktrees. Invoke when user mentions "agency", "ag", parallel tasks, worktrees, or wants to run multiple coding agents simultaneously.
---

# Agency CLI

Agency is a command-line AI agent orchestrator that runs coding agents in isolated Git worktrees with tmux-managed sessions.

## Essential Commands

### Create and Start a Task

```bash
agency new <slug>              # Create task, start session, attach immediately
ag new <slug>                  # Short alias
agency new <slug> -e           # Open editor to write task description
agency new <slug> "description"  # Provide description inline
```

### Create a Draft (No Session Yet)

```bash
agency new --draft <slug>      # Create task file only, no worktree or session
agency edit <slug>             # Edit the draft description
agency start <slug>            # Start the draft (creates worktree and session)
```

Note: The "<slug>" should only contain alphanumerical characters and "-". You can keep it relatively short.

### List Tasks

```bash
agency tasks                   # Show all tasks with status, commits, and changes
ag tasks                       # Short alias
```

Output columns: ID, SLUG, STATUS, UNCOMMITTED, COMMITS, BASE, AGENT

### Merge and Cleanup

```bash
agency merge <slug>            # Merge task branch into base, then cleanup
```

This command:

1. Rebases task branch onto base branch
2. Fast-forward merges into base
3. Deletes the worktree
4. Deletes the task branch
5. Removes the task file

### Stop Without Merging

```bash
agency stop <slug>             # Stop session, keep worktree and branch
```

## Quick Workflow Example

```bash
# Create multiple parallel tasks
agency new --draft feature-auth "Implement user authentication"
agency new --draft feature-api "Build REST API endpoints"
agency new --draft fix-tests "Fix failing unit tests"

# Start all tasks
agency start feature-auth
agency start feature-api
agency start fix-tests

# Check status
agency tasks

# When done, merge back
agency merge feature-auth
```

## More Information

Run `agency --help` for the full command reference.
