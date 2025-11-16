# Proposal: Attach follow TUI focus

Owner: agency CLI
Status: Draft
Change-ID: attach-follow-tui-focus

## Summary
Add `--follow [<tui-id>]` to `agency attach` to mirror the focused task in a running Agency TUI. Each TUI instance gets a small numeric ID (starting at 1) displayed in the Tasks frame title on the right side. When `--follow` is used without an ID, and exactly one TUI is open, attach follows that TUI; when multiple are open, it errors with a guided message. While following, the CLI manages a child process that either attaches to the focused task's tmux session or runs a minimal overlay when no session exists. On focus change, the CLI terminates the current child (attach or overlay) and starts the appropriate new child for the new focus.

## Motivation
- Reduce friction moving between TUI navigation and tmux-attached sessions.
- Make multiple TUIs distinguishable with a stable, visible ID.
- Provide clear UX when no session exists for the focused task.

## Non-Goals
- Changing existing attach behavior when `--follow` is not used.

## Risks & Mitigations
- TUI liveness tracking: managed centrally by the daemon via registered PID + periodic checks (~10s).
- Races when multiple TUIs start simultaneously: daemon assigns the lowest free ID atomically.
- Avoid reliance on an existing tmux client: the follower process owns an `attach-session` child and restarts it on focus changes. This works even when no tmux client is currently active.

## Alternatives considered
- File-based focus channel (simpler, but rejected per requirement to use daemon-managed events).
- Tmux client switching (`switch-client -c ... -t ...`): rejected because a tmux client may not be active or addressable when follow starts.

## Assumptions
- The CLI runs outside tmux when using `agency attach --follow`. The implementation always uses `tmux attach-session` children (or a fallback overlay) and never relies on `switch-client` or an existing client.
