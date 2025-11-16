## Phase 1 — Protocol scaffolding
- [x] Define C2D messages: `TuiRegister { project, pid }`, `TuiUnregister { project, pid }`, `TuiFocusTaskChange { project, tui_id, task_id }`, `TuiFollow { project, tui_id }`, `TuiList { project }`
- [x] Define D2C messages: `TuiRegistered { tui_id }`, `TuiFollowSucceeded { tui_id }`, `TuiFollowFailed { message }`, `TuiFocusTaskChanged { project, tui_id, task_id }`, `TuiList { items }`
- [x] Wire serde + validation of `project`, `tui_id`, `task_id`
- [x] Tests: encode/decode roundtrips; error on invalid inputs

## Phase 2 — Daemon registry + liveness
- [x] Per-project registry: `tui_id -> { pid, last_seen, focused_task_id: Option<u32> }`
- [x] Assign lowest free positive `tui_id` on register; handle unregister best-effort
- [x] Periodic liveness (~10s): drop dead PIDs and cleanup
- [x] Implement `TuiList` and `TuiFollow`; emit current focus immediately on follow
- [x] Broadcast `TuiFocusTaskChanged` on focus updates
- [x] Tests: id assignment/reuse, list contents, focus broadcast, liveness purge

## Phase 3 — TUI integration (minimal)
- [x] On start: `TuiRegister` and store `tui_id`; on exit: `TuiUnregister`
- [x] On selection change: send `TuiFocusTaskChange`
- [x] UI: show `TUI Id: <id>` cyan, right-aligned
- [x] Tests/manual: TUI ID visible; focus events emitted

## Phase 4 — CLI follow core (sessions only)
- [x] Clap: add `--follow [<tui-id>]` (mutually exclusive with positional task/`--session`)
- [x] Resolve TUI target: `TuiList` auto-pick single; show exact multi-TUI error
- [x] Subscribe to events; `TuiFollow` handshake; handle success/failure
- [x] DRY: extract `tmux::attach_cmd(cfg, task)` used by both `attach_session` and `spawn_attach_session`
- [x] Add `tmux::spawn_attach_session(cfg, task) -> Child` using shared builder
- [x] Implement follower child lifecycle for running sessions: spawn attach child; on focus change, terminate previous child then spawn new one
- [x] Tests: flag exclusivity; auto-pick/error messaging; attach lifecycle on focus changes

## Phase 5 — Inline no-session overlay + integration
- [x] Inline overlay: no subcommand/process; printed directly without raw mode
 - [x] Message: `No session for Task <slug> (ID: <id>). Press 's' to start, C-c to cancel.`
- [x] On `s`: reuse `utils/session::{build_session_plan, start_session_for_task}`; attach once daemon snapshot reflects the session
- [x] Follower integration: when no session → render inline overlay; on focus change or session start → update or attach child
- [x] Tests: overlay decision; action path triggers start; attach on session detection

## Phase 6 — Robustness + cancellation
- [x] Graceful child termination: SIGTERM; fallback SIGKILL after short timeout
 - [x] Cancel follow when user detaches from tmux attach child
- [x] Generation counter to avoid race conditions on late exits
- [x] Cancel follow if TUI disappears per daemon; concise error output
- [x] Tests: cancellation; race safety; termination behavior (mocked)

## Phase 7 — Docs + validation
- [x] Update README/help for `agency attach --follow [<tui-id>]` and TUI ID display
- [x] `just check`, clippy, `cargo fmt`
- [x] Add local test instructions
  - Run all tests with nextest: `just test`
  - Or: `cargo nextest run -p agency`
  - Disable autostart when needed: `AGENCY_NO_AUTOSTART=1`
- [x] 7.15 DRY checks: `attach_session` and `spawn_attach_session` use the same command builder; overlay subcommand uses `start_session_for_task`
