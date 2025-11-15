# PLAN: Unified project state and live UNCOMMITTED/COMMITS
Add UNCOMMITTED and COMMITS columns to TUI and ps backed by a single daemon stream with async metrics.

## Goals
- Add UNCOMMITTED and COMMITS columns to TUI and ps
- Compute metrics in the daemon every 1s and on status changes
- Only compute while Running or Completed; skip other statuses
- Use one combined daemon subscription carrying sessions, tasks, and metrics
- Prefer `frontmatter.base_branch`, else current HEAD
- Remove old split session/task events and client code

## Out of scope
- Multi-repo aggregation
- Persisting metrics to disk
- Computing metrics for Idle/Stopped/Exited/Draft
- Backward compatibility with legacy events

## Current Behavior
- ps shows ID, SLUG, STATUS, SESSION, BASE, AGENT (no git metrics) in `crates/agency/src/commands/ps.rs:1`.
- TUI shows ID, SLUG, STATUS, BASE, AGENT; builds rows via `build_task_rows` in `crates/agency/src/tui/mod.rs:660` and renders header at `crates/agency/src/tui/mod.rs:200`.
- Daemon broadcasts tmux sessions via `SubscribeEvents` → `SessionsChanged` every 250ms and rebroadcasts `TasksChanged` on `NotifyTasksChanged` (`crates/agency/src/daemon.rs:62, 143`).
- Protocol includes `SessionInfo` and split control messages (`crates/agency/src/daemon_protocol.rs:1`).
- No utils for diffstat or ahead counts (`crates/agency/src/utils/git.rs:1`).

## Solution
- Replace split events with one combined `ProjectState` message carrying:
  - Tasks index (id, slug, base_branch), active sessions, and metrics
- Daemon worker:
  - Poll every 1000ms and on `NotifyTasksChanged`
  - Determine candidate tasks: those Running (from tmux sessions) or Completed (flag files)
  - Compute metrics:
    - UNCOMMITTED: working tree unstaged diffstat (sum additions/deletions)
    - COMMITS: ahead count vs base (`base..branch`)
  - Cache per task and broadcast only on change
- TUI:
  - Subscribe to `ProjectState`; store sessions, tasks, metrics; render new columns
  - No per-row git calls; only present received data
- ps:
  - Request one-shot `ProjectState`; render with new columns; fallback to “-” when daemon unavailable
- Rename column label to UNCOMMITTED (correct spelling)

## Architecture
- Protocol (`crates/agency/src/daemon_protocol.rs`)
  - + struct `TaskInfo { id: u32, slug: String, base_branch: String }`
  - + struct `TaskMetrics { task: TaskMeta, uncommitted_add: u64, uncommitted_del: u64, commits_ahead: u64, updated_at_ms: u64 }`
  - + `C2DControl::ListProjectState { project: ProjectKey }`
  - = `C2DControl::SubscribeEvents { project }` now yields `ProjectState`
  - + `D2CControl::ProjectState { project: ProjectKey, tasks: Vec<TaskInfo>, sessions: Vec<SessionInfo>, metrics: Vec<TaskMetrics> }`
  - - Remove `D2CControl::{Sessions, SessionsChanged, TasksChanged}`
- Daemon (`crates/agency/src/daemon.rs`)
  - Poller at 1000ms builds `ProjectState` per subscribed project
  - Compute candidate tasks (Running via tmux; Completed via `.agency/state/completed` + `.agency/tasks`)
  - Compute/cache metrics; broadcast `ProjectState` on changes and on `NotifyTasksChanged`
  - Serve `ListProjectState` on demand
- Git utils (`crates/agency/src/utils/git.rs`)
  - + `fn uncommitted_numstat_at(workdir: &Path) -> Result<(u64,u64)>` using `git diff --numstat`
  - + `fn commits_ahead_at(repo_root: &Path, base: &str, branch: &str) -> Result<u64>` using `git rev-list --count base..branch`
- TUI (`crates/agency/src/tui/mod.rs`)
  - State: add maps for tasks and metrics from `ProjectState`
  - `TaskRow`: add `uncommitted_display: String`, `commits_display: String`
  - Header: add UNCOMMITTED and COMMITS; keep ID, SLUG, STATUS, BASE, AGENT
  - Render UNCOMMITTED with mixed colored spans (+ green, - red; gray when 0); COMMITS cyan when >0, gray when 0
  - Subscribe to `ProjectState`; remove legacy Sessions/Tasks handling
- ps (`crates/agency/src/commands/ps.rs`)
  - Request `ListProjectState` and build rows with UNCOMMITTED/COMMITS
  - If daemon unavailable, render “-” for both new columns

## Testing
- Unit (utils::git)
  - `uncommitted_numstat_at` on temp repo with unstaged adds/dels
  - `commits_ahead_at` on temp repo with base and feature diverged by known N
- Unit (TUI formatting)
  - UNCOMMITTED formatting of `+0-0` gray; non-zero mixed colors
  - COMMITS coloring (0 gray; >0 cyan)
- Integration (ps)
  - Daemon unavailable: ps prints columns including UNCOMMITTED/COMMITS with “-”
- Integration (daemon)
  - `SubscribeEvents` yields `ProjectState`; `ListProjectState` returns consistent snapshot (if feasible; otherwise unit test the assembler function)

## Detailed Plan
- [ ] Protocol: add `TaskInfo`, `TaskMetrics`, `ListProjectState`; replace split events with `ProjectState`
- [ ] Git utils: implement `uncommitted_numstat_at` and `commits_ahead_at` with unit tests
- [ ] Daemon:
  - [ ] Build task index (id, slug, base_branch) by scanning tasks and frontmatter (fallback to HEAD)
  - [ ] Identify Running tasks from tmux sessions and Completed tasks from flags
  - [ ] Compute metrics for candidates; cache; diff against last snapshot
  - [ ] Poll every 1000ms; on changes or `NotifyTasksChanged` send `ProjectState`
  - [ ] Implement `ListProjectState` handler
  - [ ] Remove legacy `Sessions`/`SessionsChanged`/`TasksChanged` code
- [ ] TUI:
  - [ ] Update `subscribe_events` to handle `ProjectState`; store tasks + sessions + metrics
  - [ ] Extend `TaskRow` and `build_task_rows` to include UNCOMMITTED and COMMITS from cached metrics
  - [ ] Update table header and constraints; render colored cells
  - [ ] Remove `list_sessions_for_project` and legacy event handling
  - [ ] Add/adjust unit tests for formatting and row building
- [ ] ps:
  - [ ] Query `ListProjectState`; build rows using same formatting
  - [ ] Fallback “-” when daemon unavailable
- [ ] Cleanup:
  - [ ] Remove `utils::daemon::list_sessions_for_project` and all call sites
  - [ ] Fix imports and delete dead code paths
- [ ] Quality:
  - [ ] `just check`
  - [ ] `cargo fmt`
  - [ ] `just check-verbose`; address warnings

## Questions
1) UNCOMMITTED spelling confirmed; using “UNCOMMITTED”.
2) UNCOMMITTED semantics: unstaged working tree diffstat of the task’s worktree via `git diff --numstat`. Assumed yes.
3) COMMITS semantics: ahead-only count vs base via `rev-list --count base..branch`. Assumed yes.
4) Status filter: compute only for Running tasks and tasks marked Completed; exclude Idle/Stopped/Exited/Draft. Assumed yes.
5) ps snapshot API: implement `ListProjectState` one-shot. Assumed yes.
6) Fallback when daemon unavailable: display “-” for both metrics; no local heavy computation. Assumed yes.
7) Interval cadence: 1000ms (1s) for the poller and change broadcasts. Assumed yes.

