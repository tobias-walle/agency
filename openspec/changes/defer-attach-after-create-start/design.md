## Context
Users run a TUI and a follower (`agency attach --follow`) in a separate terminal. When the user creates or starts a task from the TUI, attaching in the TUI terminal immediately fights with the follower and breaks focus. We keep global CLI behavior untouched and only adapt TUI-initiated flows when a follower is active.

## Goals / Non-Goals
- Goals: Preserve uninterrupted TUI workflow while being followed; let the follower attach; keep general CLI behavior unchanged
- Non-Goals: Change default CLI attach policies; add config toggles; redesign follow protocol or tmux integration

## Decisions
- When a follower is active for the current TUI: TUI-initiated create/start do not auto-attach; TUI remains active; follower attaches instead
- Outside of follower context: keep current behavior (including start-and-attach flows)

## Alternatives considered
- Disable auto-attach globally: rejected — undesired change in CLI behavior
- Config toggles to control defaults: rejected — unnecessary for this targeted fix

## Risks / Trade-offs
- Risk: Edge cases where follower is momentarily disconnected → mitigation: rely on daemon’s authoritative TUI follow state; fall back to current behavior when not followed

## Migration Plan
1. Detect follower state for the active TUI (via daemon follower events)
2. Gate TUI-initiated start-and-attach paths to skip attaching while follower is active
3. Keep focus in the TUI after start

## Open Questions
- Scope strictly to TUI-initiated actions? (proposal assumes yes per guidance)
