# PLN-3: Worktrees and new CLI commands for tasks

Date: 2025-10-26

Add Git-backed worktrees and branches for tasks and introduce `path`, `branch`, and `rm` commands with centralized task resolution, slug validation, and reusable terminal confirmation.

## Goals

- Create a branch `agency/[id]-[slug]` and a worktree `.agency/worktrees/[id]-[slug]` on `agency new [slug]`
- Add `agency path [id|slug]` to print the absolute worktree path (raw output)
- Add `agency branch [id|slug]` to print the branch name (raw output)
- Add `agency rm [id|slug]` to delete the markdown file, worktree (even if locked), and branch after `y`/`Y` confirmation
- Centralize `[id|slug]` resolution and slug validation in one place
- Disallow slugs starting with digits
- Implement Git behavior with `git2`, operating from the main repo even if CLI runs inside a worktree
- Unify tests into a single `cli.rs` and enhance `TestEnv` with repo setup helpers using a fixed commit message `"init"`

## Non Goals

- Remote operations or network credentials
- Windows support
- Bare repositories without a working directory
- Multi-repo orchestration

## Current Behavior

- `agency new [slug]` creates `.agency/tasks/[id]-[slug].md` with ID auto-increment and allows slugs with alphanumerics and `-` [crates/agency/src/commands/new.rs]
- `TaskFileName::parse()` parses `[id]-[slug].md` [crates/agency/src/utils/task.rs]
- Only `new` subcommand exists [crates/agency/src/lib.rs]
- Tests cover help and tasks dir creation [crates/agency/tests/cli_help.rs, cli_new.rs]

## Solution

- Add `git2` and implement Git helpers to:
  - Discover the repository from `cwd` and resolve main repo if currently in a worktree
  - Ensure HEAD is a non-detached commit; bail otherwise
  - Create branch `agency/[id]-[slug]` from main repo’s HEAD
  - Add a worktree named `[id]-[slug]` at `.agency/worktrees/[id]-[slug]` for that branch
- Centralize task resolution and validation in `utils/task.rs`:
  - `TaskRef { id, slug }`
  - `TaskRef::from_task_file(path)` (move logic from `TaskFileName::parse`)
  - `resolve_id_or_slug(cfg, ident) -> TaskRef` (digits-only => ID; else slug)
  - `normalize_and_validate_slug(input) -> String` (letters as first char; alnum/`-` afterward)
  - Helpers to compute branch name, worktree name, worktree dir, and task file path
- Add `utils/term.rs` with `confirm(prompt) -> bool` (y/Y only)
- Extend `AgencyConfig` with `worktrees_dir()`
- New commands: `path`, `branch`, `rm` with raw outputs for `path`/`branch` and colored, modern prompts/messages for `rm`

## Detailed Plan

HINT: Update checkboxes during the implementation

1. [x] Dependencies and modules
   - crates/agency/Cargo.toml: add `git2`
   - crates/agency/src/utils/mod.rs: export `task`, add `git` and `term` modules
   - New file crates/agency/src/utils/git.rs:
     - `open_main_repo(cwd: &Path) -> Result<git2::Repository>`
     - `ensure_branch(repo, name) -> Result<git2::Branch>`
     - `add_worktree(repo, wt_name, wt_path, branch_ref) -> Result<git2::Worktree>`
     - `remove_worktree_and_branch(repo, wt_name, branch_name) -> Result<()>`
2. [x] Centralize task resolution and validation
   - crates/agency/src/utils/task.rs:
     - Add `pub struct TaskRef { pub id: u32, pub slug: String }`
     - Add `impl TaskRef { pub fn from_task_file(path: &Path) -> Option<Self> }` (move logic from `TaskFileName::parse`)
     - Add `pub fn normalize_and_validate_slug(input: &str) -> Result<String>` (letters as first char; alnum/`-` afterward)
     - Add `pub fn resolve_id_or_slug(cfg: &AgencyConfig, ident: &str) -> Result<TaskRef>`
     - Add helpers: `branch_name(&TaskRef)`, `worktree_name(&TaskRef)`, `worktree_dir(&AgencyConfig, &TaskRef)`, `task_file(&AgencyConfig, &TaskRef)`
     - Remove `TaskFileName` type and update all references
3. [x] Config
   - crates/agency/src/config.rs: add `pub fn worktrees_dir(&self) -> PathBuf`
4. [x] Commands wiring
   - crates/agency/src/lib.rs:
     - Extend `Commands` with `Path { ident: String }`, `Branch { ident: String }`, `Rm { ident: String }`
     - Route to new modules
   - crates/agency/src/commands/mod.rs: export `path`, `branch`, `rm`
5. [ ] Implement commands
   - crates/agency/src/commands/new.rs:
     - Use centralized `normalize_and_validate_slug`
     - After writing markdown: open main repo, ensure HEAD valid, create branch `agency/[id]-[slug]`, create worktree `[id]-[slug]` at `.agency/worktrees/[id]-[slug]`
     - Print colored success for file, branch, and worktree
   - New crates/agency/src/commands/path.rs:
     - Resolve `TaskRef`, compute absolute worktree path, print raw only
   - New crates/agency/src/commands/branch.rs:
     - Resolve `TaskRef`, compute branch name, print raw only
   - New crates/agency/src/commands/rm.rs`:
     - Resolve `TaskRef` and compute file/branch/worktree
     - Print colored summary then `utils::term::confirm(...)`
     - If confirmed: open main repo, prune worktree (force even if locked), delete branch, remove task file, print colored success
     - If not: print colored cancelled message
6. [x] Reusable confirmation
   - New crates/agency/src/utils/term.rs:
     - `pub fn confirm(prompt: &str) -> Result<bool>`: y/Y => true; else false
7. [x] Tests (single file, enhanced env)
   - Replace tests with crates/agency/tests/cli.rs (merge existing)
   - crates/agency/tests/common/mod.rs:
     - Add `setup_git_repo(&self) -> Result<()>` (init repo, set HEAD to main)
     - Add `simulate_initial_commit(&self) -> Result<()>` (write file, stage, commit with message `"init"`)
   - Test cases:
     - `new_creates_markdown_branch_and_worktree`
     - `path_prints_absolute_worktree_path_by_id_and_slug`
     - `branch_prints_branch_name_by_id_and_slug`
     - `rm_requires_confirmation_and_removes_all_on_y_or_Y`
     - `new_rejects_slugs_starting_with_digits`
8. [x] Checks
   - Run `just check`, `just test`, `just fmt`
   - Ensure interactive outputs use `anstream` and `owo-colors`, and fatal paths use `bail!`

## Notes

- Used `git2` v0.20; worktrees created via `Repository::worktree` with `WorktreeAddOptions::reference(Some(&branch_ref))` and `checkout_existing(true)`.
- Always operate from main repo using `Repository::discover` + `commondir()` when in a worktree (`open_main_repo`).
- Ensured `.agency/worktrees` exists before adding worktree to avoid add errors.
- Centralized slug rules: first char must be a letter; alnum/`-` allowed afterwards; digits-only input resolves as ID.
- `path`/`branch` output is raw; interactive flows (`new`, `rm`) use `anstream` + `owo-colors` and `bail!` on fatal errors.
- Tests consolidated in `crates/agency/tests/cli.rs`; legacy tests kept temporarily for backward coverage (can remove later).
- Added `setup_git_repo` and `simulate_initial_commit` helpers with fixed commit msg `init`.
- Confirm prompt accepts only `y`/`Y`; anything else cancels.


- Always create worktrees from the main repository even if the CLI runs within a worktree.
- Bypass ambiguity by treating digits-only input as ID and prohibiting slugs that start with digits.
- Keep `path`/`branch` outputs raw for scripting, and reserve colored styling for interactive flows (e.g., `new`, `rm`).
- The `simulate_initial_commit` helper should consistently use the message `"init"`.