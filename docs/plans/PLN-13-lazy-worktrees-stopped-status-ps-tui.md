# PLAN: Lazy worktrees, Stopped status, richer ps/TUI, and keybinds
Delay worktree creation to attach time, add a "Stopped" status, show HEAD branch and agent in ps/TUI, and adjust TUI keybindings.

## Goals
- Create and bootstrap worktrees only when starting a session (attach)
- Show "Stopped" (red) when a worktree exists but no PTY session is running
- Display current HEAD branch and effective agent in ps and TUI
- TUI: 's' starts a session; 'S' stops sessions for the task (keep worktree and task)

## Out of scope
- Changing merge semantics beyond compatibility with lazy worktrees
- Auto-creating worktrees in non-attach commands (e.g., open/path)
- Daemon protocol changes or broader session lifecycle refactors

## Current Behavior
- New immediately creates a branch and worktree, then bootstraps root files and may attach
  - `crates/agency/src/commands/new.rs:59` to `:86`
- ps lists ID, SLUG, STATUS, SESSION; "Draft" when no session
  - `crates/agency/src/commands/ps.rs:12` to `:35`, `:61` to `:86`
- TUI lists ID, SLUG, STATUS, SESSION; uppercase 'S' starts a session; no stop-only key
  - `crates/agency/src/tui/mod.rs:91` to `:116`, `:190` to `:276`
- Task front matter supports `agent` and `base_branch`; new writes `base_branch` from current HEAD
  - `crates/agency/src/utils/task.rs:197` to `:216`, `:259` to `:267`
  - `crates/agency/src/commands/new.rs:36` to `:54`
- Utilities exist for sessions, worktrees, and bootstrap
  - Sessions: `crates/agency/src/utils/daemon.rs`
  - Git/worktrees: `crates/agency/src/utils/git.rs`
  - Bootstrap: `crates/agency/src/utils/bootstrap.rs`

## Solution
- Move branch/worktree creation and bootstrapping from new into attach (just-in-time)
- Status derivation in ps/TUI:
  - Active session -> use daemon status (Running/Exited)
  - No session -> if worktree dir exists -> "Stopped" (red), else "Draft" (yellow)
- ps/TUI columns:
  - BASE: show current repository HEAD branch name
  - AGENT: YAML `agent` or config default; show "-" if neither is set
- TUI keybindings:
  - 's' -> start session (start daemon if needed, then attach current task)
  - 'S' -> stop all sessions for current task (keep worktree/branch); status becomes "Stopped"

## Architecture
- Modified files
  - `crates/agency/src/commands/new.rs`
    - Remove branch/worktree creation and bootstrapping; keep writing `base_branch` from current HEAD and notifying
  - `crates/agency/src/commands/attach.rs`
    - Ensure task branch exists at recorded `base_branch` when present, else at current HEAD
    - If worktree missing: add worktree, bootstrap once, then attach
  - `crates/agency/src/commands/ps.rs`
    - Compute HEAD branch once; add BASE and AGENT columns
    - Detect worktree existence to choose between "Stopped" and "Draft" when no sessions
    - Extend color mapping to include "Stopped" -> red
  - `crates/agency/src/tui/mod.rs`
    - Extend table to include BASE and AGENT columns
    - Derive status with "Stopped" when worktree exists without session
    - Add 's' to start session; change 'S' to stop sessions only; update help text
    - Color mapping includes "Stopped" -> red
- New or extended helpers
  - `crates/agency/src/utils/git.rs`
    - `fn head_branch(ctx: &AppContext) -> String`
    - `fn ensure_branch_at(repo: &gix::Repository, name: &str, start_point: &str) -> anyhow::Result<String>`
  - `crates/agency/src/utils/task.rs`
    - `fn read_task_frontmatter(paths: &AgencyPaths, task: &TaskRef) -> Option<TaskFrontmatter>`
    - `fn agent_for_task(cfg: &AgencyConfig, fm: Option<&TaskFrontmatter>) -> Option<String>`
  - `crates/agency/src/utils/sessions.rs` (new)
    - `fn latest_sessions_by_task(sessions: &[SessionInfo]) -> HashMap<(u32, String), SessionInfo>`
  - `crates/agency/src/utils/status.rs` (new)
    - `enum TaskStatus { Draft, Stopped, Running, Exited, Other(String) }`
    - `fn derive_status(latest: Option<&SessionInfo>, worktree_exists: bool) -> TaskStatus`
  - Optional: `crates/agency/src/utils/rows.rs` (shared row building)
    - `struct TaskRowLite { id, slug, status, session, base, agent }`
    - `fn build_rows(ctx: &AppContext, tasks: &[TaskRef], sessions: &[SessionInfo]) -> Vec<TaskRowLite>`

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)

1. [ ] Add git helpers
   - Implement `head_branch(ctx)` using `open_main_repo` + `current_branch_name` with fallback to `main`
   - Implement `ensure_branch_at(repo, name, start_point)` resolving `start_point` with `rev_parse`
2. [ ] Add task metadata helpers
   - Implement `read_task_frontmatter` using `task_file` + `parse_task_markdown`
   - Implement `agent_for_task` returning front matter `agent` or config default
3. [ ] Add sessions/status utilities
   - Implement `latest_sessions_by_task`
   - Implement `TaskStatus` and `derive_status`
4. [ ] Update `new` to stop creating worktrees
   - Remove branch/worktree/bootstrapping; keep `base_branch` from HEAD and editor behavior; keep `notify_tasks_changed`
5. [ ] Update `attach` for just-in-time worktrees
   - Determine base = `frontmatter.base_branch` or `head_branch(ctx)`
   - Ensure branch at base via `ensure_branch_at`
   - If worktree missing: add worktree, bootstrap, then attach
6. [ ] Update `ps` for BASE/AGENT and "Stopped"
   - Compute head via `head_branch(ctx)`; build rows using helpers
   - Extend headers to `ID SLUG STATUS SESSION BASE AGENT`
   - Map "Stopped" to red in `get_status_text`
7. [ ] Update TUI for columns, status, and keybindings
   - Extend `TaskRow` with `base`, `agent`; compute via shared helpers
   - Add 's' to start session; set 'S' to stop sessions only; update help bar
   - Color mapping includes "Stopped" -> red
8. [ ] Update tests
   - Adjust `new_*` tests to assert worktree appears only after `attach`
   - Update bootstrap tests to run `attach` before asserting artifacts
   - Update `ps_*` tests for new headers and "Stopped" logic
   - Update TUI unit tests for `status_style` and a "Stopped" case in row building
9. [ ] Run checks and format
   - `just check` and then `just fmt`

## Questions
1. Branch creation at attach: when front matter has `base_branch`, create the task branch at that revision (not current HEAD). Assumed yes.
2. ps/TUI BASE column: show current HEAD branch at listing time. Confirmed yes.
3. AGENT display when neither is set: show "-". Assumed acceptable.
4. open/path: do not create worktrees; attach is the creation point. Assumed acceptable.
