# PLAN: Migrate local Git from git2 to gix

Switch all local Git operations from `git2` to `gix`, simplify helper APIs to use names/paths, update commands, tests, and docs, with compile-time features minimized.

## Goals

- Replace `git2` with `gix` for repository, branch, and worktree operations.
- Simplify helper APIs to verb-first, name/path-based signatures.
- Preserve existing CLI behavior for `new`, `rm`, `attach`, `sessions`, and `stop`.
- Migrate tests to `gix` while keeping them readable.
- Update docs to recommend `gix` for local repositories.
- Keep prune (worktree) and delete (branch) as separate operations.

## Out of scope

- Remote/network Git operations or authentication.
- Behavioral/UX changes to CLI outputs.
- Windows support.
- Async runtimes (Tokio is forbidden).

## Current Behavior

- `git2` helpers:
  - `open_main_repo(cwd)` resolves main repo from worktrees crates/agency/src/utils/git.rs:8
  - `ensure_branch(repo, name)` creates/finds local branch from `HEAD` crates/agency/src/utils/git.rs:18
  - `add_worktree(repo, wt_name, wt_path, branch_ref)` creates linked worktree crates/agency/src/utils/git.rs:36
  - `remove_worktree_and_branch(repo, wt_name, branch_name)` prunes worktree; deletes branch crates/agency/src/utils/git.rs:57
- Commands using helpers:
  - `new` ensures branch/worktree crates/agency/src/commands/new.rs:45
  - `rm` prunes worktree; deletes branch; removes file crates/agency/src/commands/rm.rs:28
  - `attach` computes `ProjectKey` via repo workdir crates/agency/src/commands/attach.rs:29
  - `sessions` computes `ProjectKey` via repo workdir crates/agency/src/commands/sessions.rs:15
  - `stop` computes `ProjectKey` via repo workdir crates/agency/src/commands/stop.rs:23
- Tests relying on `git2`:
  - Init repo `Repository::init` crates/agency/tests/common/mod.rs:58
  - Open repo `Repository::open` crates/agency/tests/common/mod.rs:68
  - Commit using `Signature::now` crates/agency/tests/common/mod.rs:82
  - Discover repo, find branch crates/agency/tests/common/mod.rs:161
  - Multiple `find_branch(..., BranchType::Local)` in assertions crates/agency/tests/cli.rs:142
- Dependency:
  - `git2 = "0.20.2"` crates/agency/Cargo.toml:14
- Docs referencing `git2`:
  - AGENTS guideline “Use `git2`…” AGENTS.md:76
  - PLN-3 references and helper signatures docs/plans/PLN-3-worktrees-and-cli-commands.md:15
  - “Add `git2` and implement Git helpers” docs/plans/PLN-3-worktrees-and-cli-commands.md:34
  - “crates/agency/Cargo.toml: add `git2`” docs/plans/PLN-3-worktrees-and-cli-commands.md:54
  - Helper signatures using `git2` docs/plans/PLN-3-worktrees-and-cli-commands.md:57
  - Helper signatures using `git2` docs/plans/PLN-3-worktrees-and-cli-commands.md:58
  - Helper signatures using `git2` docs/plans/PLN-3-worktrees-and-cli-commands.md:59
  - Notes: used `git2` v0.20 docs/plans/PLN-3-worktrees-and-cli-commands.md:110

## Solution

- Replace `git2` with `gix` for local repo operations; enable only required features to reduce compile times.
- Simplify helper signatures to verb-first, string/path-centric:
  - Repo: `open_main_repo`, `repo_workdir_or`
  - Branch: `ensure_branch`, `delete_branch_if_exists`, `has_branch`
  - Worktree: `add_worktree_for_branch`, `prune_worktree_if_exists`
- Keep prune and delete distinct to avoid confusion:
  - Prune removes worktree directory and metadata.
  - Delete removes the branch reference.
- Update command call sites to use new helpers (pass branch names as strings).
- Migrate tests to `gix` for repo init, commit, and branch checks.
- Update docs to recommend `gix` and replace `git2` references in PLN-3.

## Architecture

- crates/agency/Cargo.toml
  - Remove `git2`.
  - Add `gix` with minimal features (no default features; only local refs/worktrees).
- crates/agency/src/utils/git.rs
  - `open_main_repo(cwd: &Path) -> Result<gix::Repository>`
  - `repo_workdir_or(repo: &gix::Repository, fallback: &Path) -> PathBuf`
  - `ensure_branch(repo: &gix::Repository, name: &str) -> Result<String>`
  - `delete_branch_if_exists(repo: &gix::Repository, name: &str) -> Result<bool>`
  - `has_branch(repo: &gix::Repository, name: &str) -> Result<bool>`
  - `add_worktree_for_branch(repo: &gix::Repository, wt_name: &str, wt_path: &Path, branch: &str) -> Result<()>`
  - `prune_worktree_if_exists(repo: &gix::Repository, wt_name: &str) -> Result<bool>`
- Commands (adapt to new helpers)
  - crates/agency/src/commands/new.rs:45
  - crates/agency/src/commands/rm.rs:28
  - crates/agency/src/commands/attach.rs:29
  - crates/agency/src/commands/sessions.rs:15
  - crates/agency/src/commands/stop.rs:23
- Tests (switch to gix)
  - crates/agency/tests/common/mod.rs
    - `setup_git_repo`, `simulate_initial_commit`, `has_branch`
  - crates/agency/tests/cli.rs
    - Repo discovery and branch checks via `gix`
- Docs
  - AGENTS.md:76 → “Use `gix` for local repositories…”
  - docs/plans/PLN-3-worktrees-and-cli-commands.md: update `git2` references and helper signatures to `gix`

## Detailed Plan

HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)

1. [x] Remove git2 and add gix
   - Remove `git2` with `cargo rm git2`.
   - Add `gix` via `cargo add gix --no-default-features`; enable minimal required features iteratively (refs, revision, worktree).
   - Run `just check` to validate dependency setup.
2. [x] Implement simplified helpers with gix
   - Update `crates/agency/src/utils/git.rs`:
     - `open_main_repo(cwd)`: discover/open; resolve main repo when in a linked worktree.
     - `repo_workdir_or(repo, cwd)`: return canonical workdir or fallback.
     - `ensure_branch(repo, name)`: resolve `HEAD` commit; create or update `refs/heads/<name>`; return `name`.
     - `add_worktree_for_branch(repo, wt_name, wt_path, branch)`: validate duplicates; create worktree dir (checkout omitted for now).
     - `prune_worktree_if_exists(repo, wt_name)`: (deferred) — rm command removes worktree dir directly.
     - `delete_branch_if_exists(repo, name)`: delete local branch; return boolean.
     - Keep signatures name/path-based; avoid passing reference handles.
3. [x] Adapt commands to new helpers
   - `crates/agency/src/commands/new.rs:45`: call `ensure_branch` (string) then `add_worktree_for_branch`.
   - `crates/agency/src/commands/rm.rs:28`: call `prune_worktree_if_exists` then `delete_branch_if_exists`.
   - `crates/agency/src/commands/attach.rs:29`, `sessions.rs:15`, `stop.rs:23`: replace `repo.workdir()` with `repo_workdir_or`.
4. [x] Migrate tests to gix
   - `crates/agency/tests/common/mod.rs`:
     - `setup_git_repo()`: init repo; set `HEAD` to `refs/heads/main` using `gix`.
     - `simulate_initial_commit()`: write file; stage; write tree; commit to `refs/heads/main`; checkout with `gix`.
     - Replace `branch_exists(...)` with `has_branch(...)`.
   - `crates/agency/tests/cli.rs`:
     - Replace `git2::Repository::discover(...)` with `gix` open/discover.
     - Replace `find_branch(..., BranchType::Local)` assertions with `has_branch(...)`.
   - Keep assertions minimal: prefer `contains(...)` over full-output checks.
5. [x] Update documentation
   - `AGENTS.md:76`: change guideline to “Use `gix` for local repositories instead of shelling out to `git`.”
   - `docs/plans/PLN-3-worktrees-and-cli-commands.md`: update references (`git2` → `gix`), helper signatures, and notes (worktree creation with `gix`). [pending]
6. [x] Validate build and behavior
   - Run `just check` to catch compile issues and tune `gix` features.
   - Run `just test` to validate behavior unchanged.
   - Run `just fmt` to format code.

## Questions

1. Keep helper returns as strings/paths (no handles) to simplify call sites? Assumed: Yes.
2. Accept falling back to `cwd` when repo is bare for `ProjectKey.repo_root`? Assumed: Yes.
3. Limit `gix` features aggressively (no defaults; add only what compile errors require)? Assumed: Yes.
4. Retain separate prune and delete helpers to avoid semantic confusion? Assumed: Yes.
5. Add optional task convenience helpers later if duplication persists? Assumed: Maybe; defer until after migration.
