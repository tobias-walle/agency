# PLAN: Bootstrap git-ignored root files into new worktrees
Copy git-ignored root files and explicitly included root directories into each new worktree using reflink with fallback to regular copy, configured via merged include/exclude lists and a 10MB per-file cap.

## Goals
- Copy all git-ignored files at the repo’s root workdir (unless excluded)
- Copy root-level directories only if explicitly included by config
- Enforce a fixed 10MB per-file size limit; skip larger files
- Preserve isolation; never use symlinks or hardlinks
- Use reflink first; fall back to regular copy on failure
- Merge `bootstrap.include` and `bootstrap.exclude` across defaults, XDG, and project configs
- Integrate with `gix` helpers; avoid shelling out to `git` for ignore checks

## Out of scope
- Copying files from nested subdirectories unless their parent dir is explicitly included
- Additional config keys beyond `bootstrap.include` and `bootstrap.exclude`
- Windows support or networked caches

## Current Behavior
- Worktree creation and branch setup
  - Entrypoint creates task markdown, ensures branch, adds worktree: crates/agency/src/commands/new.rs:66
  - Base branch resolution: crates/agency/src/commands/new.rs:35
  - Current branch helper: crates/agency/src/utils/git.rs:109
- Git helpers and worktree operations (migrated to `gix`)
  - Discover/open main repo: crates/agency/src/utils/git.rs:8
  - Ensure branch: crates/agency/src/utils/git.rs:26
  - Add worktree for branch: crates/agency/src/utils/git.rs:85
- Config loading and merging
  - Defaults embed + merge XDG + project: crates/agency/src/config.rs:117
  - Merge behavior currently replaces arrays (last-wins): crates/agency/src/config.rs:110
- Result: new worktrees start without untracked/ignored root files (e.g., `.env`) or ignored directories.

## Solution
- Add a bootstrap step immediately after worktree creation
  - Scope: only entries in the main repo’s root workdir
  - Determine git-ignored status via `gix` ignore evaluator (respect `.gitignore`, `.git/info/exclude`, global excludes)
  - Copy policy
    - Files: copy if git-ignored, not excluded by name, and size ≤ 10MB
    - Directories: copy only if the directory name is listed in `bootstrap.include`; copy entire tree of that directory
  - Exclusions: always exclude `.git` and `.agency`; merge with user-configured `bootstrap.exclude`
  - Copy method: use `reflink_copy` (APFS/Btrfs) per file; on failure, fall back to `std::fs::copy`
  - Idempotency: skip entries if destination already exists; handle partial copies safely
- Config arrays merging
  - Introduce `bootstrap.include` and `bootstrap.exclude` and merge arrays across config layers (concatenate + deduplicate) rather than overriding
- Naming and style
  - Verb-first function names; remove `_cow` suffixes
  - Use `root_workdir` identifier for clarity

## Architecture
- Files and symbols
  - crates/agency/src/utils/bootstrap.rs
    - `pub fn bootstrap_worktree(repo: &gix::Repository, root_workdir: &Path, dst_worktree: &Path, cfg: &BootstrapConfig) -> Result<()>`
    - `fn discover_root_entries(root_workdir: &Path) -> Result<Vec<std::fs::DirEntry>>`
    - `fn evaluate_ignore(repo: &gix::Repository, entry_path: &Path) -> Result<bool>`
    - `fn is_excluded(entry_name: &str, cfg: &BootstrapConfig) -> bool`
    - `fn copy_file(src: &Path, dst: &Path) -> Result<()>` (try reflink, fallback to copy)
    - `fn copy_dir_tree(src_dir: &Path, dst_dir: &Path) -> Result<()>` (walk + per-file copy)
    - `fn file_size_within_limit(path: &Path) -> Result<bool>` (≤ 10MB constant)
    - Const: `MAX_BOOTSTRAP_FILE_BYTES: u64 = 10 * 1024 * 1024`
  - crates/agency/src/config.rs
    - `#[derive(Default, Deserialize, Clone)] pub struct BootstrapConfig { pub include: Vec<String>, pub exclude: Vec<String> }`
    - `impl AgencyConfig { pub fn bootstrap_config(&self) -> BootstrapConfig }` (returns merged lists with defaults)
    - Adjust `merge_values` to concatenate arrays for `bootstrap.include` and `bootstrap.exclude` (dedup) while keeping replace semantics elsewhere
  - crates/agency/defaults/agency.toml
    - `[bootstrap]`
      - `include = []`  // user extends, e.g., `[".venv", ".direnv"]`
      - `exclude = [".git", ".agency"]`
  - crates/agency/src/commands/new.rs
    - After `add_worktree_for_branch(...)`, call `bootstrap_worktree(repo, root_workdir, wt_dir, &ctx.config.bootstrap_config())`
    - Use `anstream::println` + `owo-colors` for concise output
- Dependencies (added via `cargo add`)
  - `reflink-copy` (reflink-first copies)
  - `walkdir` (copy files inside included directories)
  - `fs-err` (optional: clearer fs errors)
  - `rayon` (optional: parallel file copies in big included dirs)

## Detailed Plan
HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)
1. [ ] Add CLI tests for default behavior (root files only)
   - Set up temp repo under `crates/agency/target/test-tmp` with `.gitignore` ignoring `.env`, `.env.local`, `secrets.txt`, `.venv/`, `.direnv/`
   - Place root files: `.env`, `.env.local`, `secrets.txt` (<10MB), `big.bin` (≥10MB, ignored)
   - Place root dirs: `.venv/` and `.direnv/` with small files
   - Initialize repo and first commit via `gix`
   - Run `agency new test` and assert in the new worktree:
     - Present: `.env`, `.env.local`, `secrets.txt`
     - Absent: `big.bin` (size limit), `.venv/`, `.direnv/`, `.git`, `.agency`
2. [ ] Add CLI tests for config inclusion/exclusion merging
   - Project config `.agency/agency.toml`: `[bootstrap] include = [".venv"]`
   - XDG config (via `temp-env` `XDG_CONFIG_HOME`): `agency/agency.toml` with `[bootstrap] include = [".direnv"]` and `exclude = [".env.local"]`
   - Assert merged results in worktree:
     - Present: `.env`, `secrets.txt`, `.venv/` tree, `.direnv/` tree
     - Absent: `.env.local`, `.git`, `.agency`
3. [ ] Implement `BootstrapConfig` and merged accessor
   - Extend `AgencyConfig` with `bootstrap: Option<BootstrapConfig>`
   - Implement `bootstrap_config()` to return merged include/exclude lists with defaults
   - Update embedded defaults in `crates/agency/defaults/agency.toml`
4. [ ] Adjust `merge_values` for array concatenation
   - Special-case `bootstrap.include` and `bootstrap.exclude` to concatenate and deduplicate across defaults, XDG, and project configs
   - Keep existing replace semantics for all other arrays/scalars
5. [ ] Implement bootstrap logic
   - Add `utils/bootstrap.rs` with verb-first functions
   - Enumerate `root_workdir` entries; for files, copy if ignored, not excluded, and ≤ 10MB
   - For dirs, copy only if name is in `cfg.include`; traverse and copy per file
   - Use `reflink_copy::reflink_or_copy` for files; for directories, apply per-file
   - Idempotent checks and robust error contexts; colorful summary logs
   - Use `gix` ignore APIs to avoid manual parsing of `.gitignore`
6. [ ] Wire into `agency new`
   - After worktree creation, compute `root_workdir` via `repo_workdir_or(...)` and call `bootstrap_worktree(...)`
7. [ ] Docs and validation
   - README: document bootstrap behavior, 10MB cap, and config merging
   - Run `just check` and `just fmt`; fix clippy warnings

## Questions
1) Default excludes are `[".git", ".agency"]`. Any others to include by default or keep minimal? Assumed: keep minimal.
2) The 10MB cap is hardcoded (no config knob). Assumed: yes.
3) Merge arrays across config layers by concatenation and deduplication; order not significant. Assumed: yes.
4) Only consider the root workdir; do not scan nested subdirs unless their parent dir is explicitly included. Assumed: yes.
5) Use `gix` ignore evaluator for accurate Git semantics; do not shell out. Assumed: yes.

