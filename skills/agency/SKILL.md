---
name: agency
description: Use Agency CLI to run parallel AI coding tasks in isolated Git worktrees. Invoke when user mentions "agency", "ag", parallel tasks, worktrees, or wants to run multiple coding agents simultaneously.
---

# Agency CLI

Agency is a command-line AI agent orchestrator that runs coding agents in isolated Git worktrees with tmux-managed sessions.

## Essential Commands

### Create and Start a Task

```bash
agency new <slug>                     # Create task, start session, attach immediately
ag new <slug>                         # Short alias
agency new <slug> -e                  # Open editor to write task description
agency new <slug> -f spec.pdf         # Attach a file during creation
agency new <slug> -f a.png -f b.pdf   # Attach multiple files

# Provide description inline (use heredoc for multi-line)
agency new <slug> <<'EOF'
Your task description here.
Can span multiple lines without escaping issues.
EOF
```

### Create a Draft (No Session Yet)

```bash
agency new --draft <slug>      # Create task file only, no worktree or session
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

### Delete a Task

```bash
agency rm --yes <slug>         # Remove task file, worktree, and branch
agency reset <slug>            # Reset worktree and branch, keep task file
```

Use `rm` when you want to completely delete a task without merging changes. Use `reset` if you want to start over but keep the task description.

## Attach Files to Tasks

Files can be attached to a task to provide additional context.

```bash
agency new <slug> -f path/to/file.pdf   # Attach file during creation
agency files add <slug> path/to/file    # Add file to existing task
agency files add <slug> --from-clipboard # Paste image from clipboard
agency files list <slug>                 # List attached files
agency files rm <slug> <file-id>         # Remove a file
agency info                              # Show task context and files (inside session)
```

When files are attached, agents can run `agency info` to get the context. Files are accessible insie task worktrees via `.agency/local/files/`.

Just SHOULD use files:

- Then the user mentions relevant files associated with the tasks
- Then there are relevant documents (like markdown plans) that are not version controlled or committed in the base branch of the task, they should be included like this.

E.g. the user created a plan that is not committed and asks you to delegate the task to multiple agency tasks. Then you would attach the plan to each delegated task.

## Quick Workflow Example

```bash
# Create multiple parallel tasks
agency new --draft feature-auth <<'EOF'
Implement user authentication
EOF
agency new --draft feature-api <<'EOF'
Build REST API endpoints
EOF
agency new --draft fix-tests <<'EOF'
Fix failing unit tests
EOF

# Start all tasks
agency start --no-attach feature-auth
agency start --no-attach feature-api
agency start --no-attach fix-tests

# Or start tasks on creation
agency new --no-attach feature-auth <<'EOF'
Implement user authentication
EOF
agency new --no-attach feature-api <<'EOF'
Build REST API endpoints
EOF
agency new --no-attach fix-tests <<'EOF'
Fix failing unit tests
EOF

# Check status
agency tasks

# When done, merge back
agency merge feature-auth
```

## Sandboxing

**CRITICAL: ALL `agency` commands MUST be run outside the sandbox.**

If you are running in a sandbox you will get errors like `run/agency-tmux.sock (Operation not permitted)`.

IMPORTANT: Despite the error message, this is not because the daemon is not started. It is because the sandbox cannot access the tmux socket file. You MUST run ALL agency commands outside the sandbox:

- agency new ...
- agency start ...
- agency stop ...
- agency tasks ...
- agency merge ...
- agency rm ...
- agency reset ...
- ALL other agency commands

## More Information

Run `agency --help` for the full command reference.
