# PLAN: notify-after-task-change-helper
Centralize task mutation notifications so the TUI stays in sync after merges and other task edits.

## Goals
- Ensure `agency merge` and other task-mutating commands always trigger a `TasksChanged` broadcast so the TUI refreshes immediately.
- Replace scattered `notify_tasks_changed` calls with a single guarded helper that emits exactly one notification on success.
- Add regression and unit tests that observe notification counts to prevent future omissions.

## Out of scope
- Introducing filesystem watchers or additional daemon protocol changes.
- Altering session (`SessionsChanged`) broadcasting logic.
- Redesigning broader TUI behavior or command UX beyond notification consistency.

## Current Behavior
- `crates/agency/src/commands/merge.rs:78-94` deletes the worktree, branch, and markdown file after a merge but never informs the daemon, leaving the TUI with stale status.
- Commands such as `new` (`crates/agency/src/commands/new.rs:70-74`), `reset` (`crates/agency/src/commands/reset.rs:33-36`), and both `rm` paths (`crates/agency/src/commands/rm.rs:32-60`) each call `notify_tasks_changed` manually, duplicating fragile logic.
- The notifier in `crates/agency/src/utils/daemon.rs:48-56` is public and simply shells a control frame; nothing enforces that callers remember to invoke it.
- The TUI background task creation threads still call the notifier directly (`crates/agency/src/tui/mod.rs:439-463`), further scattering the responsibility.

## Solution
- Add `notify_after_task_change(ctx, op)` in `utils::daemon`; run the provided closure, and on `Ok` emit one task notification while recording test metrics under `cfg(test)`.
- Make the low-level `notify_tasks_changed` function private so all future callers must go through the helper.
- Refactor `merge`, `new`, `reset`, both `rm` entry points, and the TUI’s task-creation threads to rely on `notify_after_task_change`, removing duplicated notifier calls.
- Expose lightweight test-only counters to assert notification emission in unit and integration tests.

## Architecture
- `crates/agency/src/utils/daemon.rs`
  - `notify_after_task_change(ctx, op)` helper and private `notify_tasks_changed`.
  - `#[cfg(test)]` metrics (`TASK_NOTIFY_COUNT`, `reset_task_notify_metrics`, `task_notify_count`).
- `crates/agency/src/commands/merge.rs`
  - Wrap merge workflow inside `notify_after_task_change`.
- `crates/agency/src/commands/{new,reset,rm}.rs`
  - Replace inline notifier calls with the helper (including `run_force`).
- `crates/agency/src/tui/mod.rs`
  - Remove direct calls to the notifier in background threads once commands cover notifications.
- `crates/agency/tests/task_notifications.rs` (new)
  - Regression tests for `notify_after_task_change` and `agency merge` notification behavior.

## Detailed Plan
- [ ] Introduce `#[cfg(test)]` metrics in `crates/agency/src/utils/daemon.rs` to observe notifier usage:
  ```rust
  #[cfg(test)]
  static TASK_NOTIFY_COUNT: AtomicU64 = AtomicU64::new(0);

  #[cfg(test)]
  pub(crate) fn reset_task_notify_metrics() {
    TASK_NOTIFY_COUNT.store(0, Ordering::SeqCst);
  }

  #[cfg(test)]
  pub(crate) fn task_notify_count() -> u64 {
    TASK_NOTIFY_COUNT.load(Ordering::SeqCst)
  }
  ```
- [ ] Implement `notify_after_task_change(ctx, op)` in the same module, make `notify_tasks_changed` private, and increment the counter when notifications are sent under `cfg(test)`.
- [ ] Refactor command entry points (`crates/agency/src/commands/merge.rs`, `new.rs`, `reset.rs`, `rm.rs`) so their task mutations execute inside `notify_after_task_change`. Ensure both paths in `rm` use the helper.
- [ ] Update the TUI background threads in `crates/agency/src/tui/mod.rs` to stop calling the notifier directly; rely on the command helpers and keep only log handling there.
- [ ] Add `crates/agency/tests/task_notifications.rs` with:
  ```rust
  #[test]
  fn helper_emits_once_on_success() {
    reset_task_notify_metrics();
    let ctx = test_app_context()?; // helper returning AppContext
    notify_after_task_change(&ctx, || Ok::<_, anyhow::Error>(()))?;
    assert_eq!(task_notify_count(), 1);
  }

  #[test]
  fn merge_triggers_notification() {
    reset_task_notify_metrics();
    let env = TestEnv::new();
    env.init_repo()?;
    // create and merge a task using CLI helpers
    let (id, slug) = env.new_task("alpha", &[])?;
    // fast-forward main to task branch etc., then run merge
    env.bin_cmd()?.args(["merge", &id.to_string()]).assert().success();
    assert_eq!(task_notify_count(), 1);
  }
  ```
  Include a failure-path test that ensures errors prevent notification.
- [ ] Run `just check`, `just test`, and `cargo fmt` to verify compilation, linting, and formatting.

## Questions
1) Should `notify_after_task_change` continue to treat notification transport errors as non-fatal (best-effort)? *Assumed yes; helper should log/ignore failures to match current behavior.*
2) Is it acceptable to rely on command-level notifications so the TUI threads no longer send their own? *Assumed yes; background threads merely launch commands.*
