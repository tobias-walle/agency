# PLAN: Merge Back and Open Worktree
Add `agency merge <id|slug>` to rebase and fast-forward merge task changes back to the base branch, then clean up the task (worktree, branch, file) on success. Add `agency open <id|slug>` to open the worktree folder via `$EDITOR`, factoring editor logic into `utils::editor`.

## Goals
- Implement `agency merge <id|slug> [-b|--branch <name>]` with robust rebase/ff logic.
- On successful merge: delete the worktree, branch, and task file.
- Implement `agency open <id|slug>` to open the worktree folder using `$EDITOR`.
- Factor editor spawn logic into `utils::editor` and reuse from `new` and `open`.
- Avoid switching HEAD in main repo for fast-forward updates.

## Out of scope
- Remote pushes or PRs.
- Auto conflict resolution.
- Partial merges or cherry-picks.
- Post-merge branch protection or automation.

## Current Behavior
- Front matter includes `base_branch` during `new` based on current HEAD or `main` (`crates/agency/src/commands/new.rs:36`, `:45`, `:50`).
- Worktree and branch creation for each task (`utils/git.rs:48` ensure_branch, `:73` add_worktree_for_branch).
- Helpers for task resolution and paths (`utils/task.rs:101` resolve_id_or_slug, `:139` branch_name, `:147` worktree_dir, `:153` task_file).
- No `merge` or `open` commands wired yet (`crates/agency/src/commands/mod.rs`, `lib.rs`).
- `$EDITOR` spawn logic exists inside `new.rs` for opening a file.

## Solution
- `merge` command:
  - Resolve task by `<id|slug>`, get task branch, worktree dir, and read base branch from front matter (fallback to `main`; allow `-b/--branch` override).
  - Rebase in worktree: `git rebase <base>`; on conflicts, bail with clear guidance to run `agency open` and fix manually, then rerun `merge`.
  - Fast-forward base to task head without switching HEAD:
    - Verify fast-forward condition: `git merge-base --is-ancestor <base_head> <task_head>`.
    - Update base ref directly: `git update-ref refs/heads/<base> <task_head>`.
  - On success, clean up: prune worktree, delete branch, delete task file.
  - Retry loop (up to 3) if fast-forward fails due to base moving between steps.
- `open` command:
  - Resolve task, compute worktree dir, and open directory via `$EDITOR` (reused helper).
- Editor helper:
  - Move `$EDITOR` tokenization and spawn to `utils::editor::open_path(&Path)`.
  - Use same behavior for file and directory paths.
- UX:
  - Colorful status messages via `anstream::println` and `owo-colors`.
  - Actionable errors via `bail!` with conflict hints.

## Architecture
- Modified files
  - `crates/agency/src/lib.rs`
    - Add `Merge { ident, #[arg(short='b', long="branch")] base: Option<String> }` and `Open { ident }` subcommands.
  - `crates/agency/src/commands/new.rs`
    - Replace local `open_editor` with `utils::editor::open_path`.
  - `crates/agency/src/utils/git.rs`
    - Add helpers:
      - `rebase_onto(worktree_dir: &Path, base: &str) -> Result<()>` using `run_git`.
      - `is_fast_forward(repo: &Repository, base: &str, task_branch: &str) -> Result<bool>` using CLI `merge-base --is-ancestor`.
      - `update_branch_ref(repo: &Repository, branch: &str, new_commit: ObjectId) -> Result<()>` using gix ref update or `git update-ref`.
      - Keep existing `prune_worktree_if_exists` and `delete_branch_if_exists` for cleanup.
- New files
  - `crates/agency/src/commands/merge.rs`
    - Implements rebase and fast-forward logic, conflict detection, retries, and cleanup.
  - `crates/agency/src/commands/open.rs`
    - Opens worktree directory via `$EDITOR`.
  - `crates/agency/src/utils/editor.rs`
    - `open_path(path: &Path) -> Result<()>` with `$EDITOR` tokenization and spawn.

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)

1. [ ] Wire CLI subcommands in `crates/agency/src/lib.rs` for `Merge` and `Open`.
2. [ ] Factor editor helper to `crates/agency/src/utils/editor.rs` and update `new.rs` to use it.
3. [ ] Implement `open` command in `crates/agency/src/commands/open.rs` to open the worktree directory.
4. [ ] Add git helpers in `crates/agency/src/utils/git.rs` for rebase, ff check, and ref update.
5. [ ] Implement `merge` command in `crates/agency/src/commands/merge.rs` with rebase, ff update, retries, and cleanup of worktree, branch, and task file.
6. [ ] Wire modules in `crates/agency/src/commands/mod.rs` and verify compile.
7. [ ] Add tests in `crates/agency/tests/cli.rs` for fast-forward merge success, conflict handling, and `open` behavior.
8. [ ] Run `just check` and `just fmt`, fix warnings and formatting.

## Questions
1) After successful merge, should we delete worktree, branch, and task file?
- Assumed: Yes. Cleanup on success is part of the command.

2) Where should the editor helper live?
- Assumed: `utils::editor` with a shared `open_path(&Path)` function.

3) Can we fast-forward without switching HEAD in the root repo?
- Assumed: Yes. Verify fast-forward and update the base branch ref directly via `git update-ref` or gix APIs, avoiding `git checkout` and keeping the current HEAD untouched.

