# PLN-7: Git Worktrees Implementation

## Context

Agency isolates each task into its own Git worktree under `./.agency/worktrees/{id}-{slug}`.
Currently, `task.start` only creates the directory and spawns a PTY.
There is no real Git worktree/branch created, and no cleanup on completion/merge.
We must implement real worktree lifecycle linked to the current repository.

## Goals

- Create a real Git worktree for each task from the current project repository.
- Create or reuse a task branch `agency/{id}-{slug}` at the base branch tip.
- Ensure `task.start` validates `base_branch`, creates the worktree, and spawns PTY in it.
- Emit a clear error if not in a Git repository.
- Provide CLI helpers: `agency path <id|slug>` and `agency shell-hook`.
- Add tests to validate correct Git state (branch, HEAD, base tip) and idempotency.

## Non-Goals

- Implement merge/squash behavior end-to-end (planned separately).
- Remote fetch/fast-forward enforcement (can be a later enhancement).

## Design

- `adapters/git.rs`
  - `ensure_task_worktree(repo, root, id, slug, base_branch) -> Result<PathBuf>`
    - Resolve `base_sha` using `resolve_base_branch_tip`.
    - Create local branch `agency/{id}-{slug}` at `base_sha` if missing.
    - Add a Git worktree at `./.agency/worktrees/{id}-{slug}` checked out to the task branch.
    - If the worktree path exists:
      - Verify it is a valid Git worktree and on the expected branch.
      - If inconsistent, return an error that explains the mismatch.
  - `remove_task_worktree(repo, root, id, slug) -> Result<()>`
    - Safely remove the worktree and delete the local task branch if appropriate.

- `daemon::task.start`
  - After validating base branch and flipping status to `running`, call `ensure_task_worktree`.
  - Spawn PTY with `cwd` set to the returned worktree path.
  - If `git2::Repository::open(root)` fails, return a user-facing error: "not a git repository".

- CLI
  - `agency path <id|slug>` prints the resolved worktree path `./.agency/worktrees/{id}-{slug}`.
  - `agency shell-hook` prints a simple function to `cd` into the task worktree by id/slug.

## Error Handling

- Not a git repo: `task.start` returns an error clearly stating the project root is not a Git repository.
- Inconsistent state: return actionable errors (e.g., worktree dir exists but is not a Git worktree, branch mismatch).

## Testing (TDD)

- Integration test: `task.start` creates a valid worktree.
  - The worktree is openable via `git2::Repository::open`.
  - `HEAD` is at branch `agency/{id}-{slug}`.
  - The branch starts at the resolved `base_sha`.
- Idempotency: calling `task.start` again reuses consistent state.
- Error when run outside a Git repo (init a temp dir without `git init`).
- Optional: Simulate inconsistent state to verify clear errors.

## Steps

1. Add worktree lifecycle helpers in `adapters/git.rs`.
2. Wire `ensure_task_worktree` into `task.start`.
3. Add CLI `path` and `shell-hook` commands.
4. Add tests for worktree creation, idempotency, and non-git error.
5. Follow-ups: cleanup on merge and enhanced resume validation.
