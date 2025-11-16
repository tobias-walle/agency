## Phase 1 — Protocol scaffolding
- [ ] Define C2D messages: `TuiRegister { project, pid }`, `TuiUnregister { project, pid }`, `TuiFocusTaskChange { project, tui_id, task_id }`, `TuiFollow { project, tui_id }`, `TuiList { project }`
- [ ] Define D2C messages: `TuiRegistered { tui_id }`, `TuiFollowSucceeded { tui_id }`, `TuiFollowFailed { message }`, `TuiFocusTaskChanged { project, tui_id, task_id }`, `TuiList { items }`
- [ ] Wire serde + validation of `project`, `tui_id`, `task_id`
- [ ] Tests: encode/decode roundtrips; error on invalid inputs

## Phase 2 — Daemon registry + liveness
- [ ] Per-project registry: `tui_id -> { pid, last_seen, focused_task_id: Option<u32> }`
- [ ] Assign lowest free positive `tui_id` on register; handle unregister best-effort
- [ ] Periodic liveness (~10s): drop dead PIDs and cleanup
- [ ] Implement `TuiList` and `TuiFollow`; emit current focus immediately on follow
- [ ] Broadcast `TuiFocusTaskChanged` on focus updates
- [ ] Tests: id assignment/reuse, list contents, focus broadcast, liveness purge

## Phase 3 — TUI integration (minimal)
- [ ] On start: `TuiRegister` and store `tui_id`; on exit: `TuiUnregister`
- [ ] On selection change: send `TuiFocusTaskChange`
- [ ] UI: show `TUI Id: <id>` cyan, right-aligned
- [ ] Tests/manual: TUI ID visible; focus events emitted

## Phase 4 — CLI follow core (sessions only)
- [ ] Clap: add `--follow [<tui-id>]` (mutually exclusive with positional task/`--session`)
- [ ] Resolve TUI target: `TuiList` auto-pick single; show exact multi-TUI error
- [ ] Subscribe to events; `TuiFollow` handshake; handle success/failure
- [ ] DRY: extract `tmux::attach_cmd(cfg, task)` used by both `attach_session` and `spawn_attach_session`
- [ ] Add `tmux::spawn_attach_session(cfg, task) -> Child` using shared builder
- [ ] Implement follower child lifecycle for running sessions: spawn attach child; on focus change, terminate previous child then spawn new one
- [ ] Tests: flag exclusivity; auto-pick/error messaging; attach lifecycle on focus changes

## Phase 5 — No-session overlay subcommand + integration
- [ ] Subcommand: `agency overlay no-session --task <id|slug>` minimal ratatui app
 - [ ] Message: `No session for Task <slug> (ID: <id>). Press s to start.`
- [ ] On `s`: reuse `utils/session::{build_session_plan, start_session_for_task}`; exit on success
- [ ] Follower integration: when no session → spawn overlay; on overlay exit/session start → spawn attach child
- [ ] DRY: overlay uses shared TUI styles (`tui/colors.rs`) and common input helpers
- [ ] Tests: overlay decision; action path triggers start; follower spawns attach

## Phase 6 — Robustness + cancellation
- [ ] Graceful child termination: SIGTERM; fallback SIGKILL after short timeout
- [ ] Generation counter to avoid race conditions on late exits
- [ ] Cancel follow if TUI disappears per daemon; concise error output
- [ ] Tests: cancellation; race safety; termination behavior (mocked)

## Phase 7 — Docs + validation
- [ ] Update README/help for `agency attach --follow [<tui-id>]` and TUI ID display
- [ ] `just check`, clippy, `cargo fmt`
- [ ] If local tests required: instructions `just test` / `cargo nextest run -p agency` and env `AGENCY_NO_AUTOSTART=1`
- [ ] 7.15 DRY checks: `attach_session` and `spawn_attach_session` use the same command builder; overlay subcommand uses `start_session_for_task`
