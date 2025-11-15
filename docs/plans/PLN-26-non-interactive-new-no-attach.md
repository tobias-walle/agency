# PLAN: Non-interactive task creation and start-detach

Add description arg to `new`, support `--no-attach` on `new` and `start`, remove `--no-edit`, and update tests to be non-interactive by default.

## Goals
- Add optional description to `new` via second positional and `--description`.
- Add `--no-attach` to `new` and `start` to start without attaching.
- Remove `--no-edit` from CLI and code.
- Refactor `start` to separate start from attach for reuse/testing.
- Update tests to avoid interactivity by default (fixed description in helper).

## Out of scope
- TUI changes or UX beyond these flags.
- Full deduplication between `start` and `attach` (keep focused changes).
- Changes to daemon protocol or session listing.

## Current Behavior
- CLI defines `new` with `slug`, `-a/--agent`, `--draft`, and `--no_edit` (crates/agency/src/lib.rs:27-39).
- `new` (crates/agency/src/commands/new.rs:17-82):
  - If stdout is a TTY and not `--no-edit`, opens editor and bails on empty description; else writes empty body immediately.
  - Always logs `Create task <slug> (id <id>)` after writing.
- After creation, `lib.rs` starts and attaches unless `--draft` (crates/agency/src/lib.rs:139-147).
- `start` ensures branch/worktree, builds env/argv, starts tmux session, then attaches (crates/agency/src/commands/start.rs:22-107).
- Tests call `new` with `--no-edit` to avoid opening an editor and use `--draft` by default via helper (crates/agency/tests/cli.rs:multiple; crates/agency/tests/common/mod.rs:106-152).

## Solution
- CLI changes
  - `new`: add `desc: Option<String>` as a second positional and `--description <text>` alias; add `--no-attach` (conflicts with `--draft`). Remove `--no-edit`.
  - `start`: add `--no-attach`.
- `new.rs`
  - Update signature to accept `desc: Option<&str>`; if provided, set body from it and bypass editor; else keep current TTY-based editor vs direct-write behavior. Keep the editor-only empty-description bail.
- `start.rs`
  - Extract `run_with_attach(ctx, ident, attach: bool)`; call `tmux::start_session(...)` always, and conditionally attach. Make `run(ctx, ident)` call `run_with_attach(ctx, ident, true)`.
- Wire `new` execution
  - Default: create -> start -> attach.
  - `--draft`: create only.
  - `--no-attach`: create -> start -> return (no attach).
- Tests
  - Remove `--no-edit` everywhere.
  - Update `TestEnv::new_task` to append `--description "Automated test"` unless caller already supplied a `--description` (keeps tests non-interactive regardless of TTY).
  - Add a test asserting that `--description` content is persisted in the markdown body.
  - Optionally add a `--no-attach` flow test, skipped when tmux isn’t available (mirroring existing socket checks).

## Architecture
- Modify
  - crates/agency/src/lib.rs
    - Commands::New: add `desc: Option<String>`, `#[arg(long = "description")] description: Option<String>`, and `#[arg(long = "no-attach", conflicts_with = "draft")] no_attach: bool`; remove `no_edit: bool`.
    - Commands::Start: add `#[arg(long = "no-attach")] no_attach: bool`.
    - Match arms: pass `desc.or(description)` to `new::run`, and use `start::run_with_attach(&ctx, &ident, !no_attach)` for `new` and `start` flows.
  - crates/agency/src/commands/new.rs
    - Change `run(ctx, slug, no_edit, agent)` to `run(ctx, slug, agent, desc)`; remove `no_edit` logic; set body from `desc` when present.
  - crates/agency/src/commands/start.rs
    - Add `pub fn run_with_attach(ctx, ident, attach: bool) -> Result<()>` and make `run` call it with `attach = true`.
  - crates/agency/tests/common/mod.rs
    - Enhance `TestEnv::new_task` to auto-append `--description "Automated test"` if caller didn’t include one.
  - crates/agency/tests/cli.rs
    - Remove `--no-edit`; add one test verifying description persistence.

## Testing
- Unit
  - Optional small unit to ensure `run_with_attach` respects `attach` flag (can be inferred through structuring; not critical).
- Integration
  - Existing tests pass without `--no-edit` (non-TTY path writes body without editor).
  - New test: `new` with `--draft --description "Automated test body"` writes body to markdown.
  - Optional: `new --no-attach` starts without attaching; skip if tmux not available.

## Detailed Plan
- [ ] Update CLI structs (crates/agency/src/lib.rs)
  - Remove `no_edit` from `Commands::New`.
  - Add `desc: Option<String>` and `#[arg(long = "description")] description: Option<String>` to `New`.
  - Add `#[arg(long = "no-attach", conflicts_with = "draft")] no_attach: bool` to `New`.
  - Add `#[arg(long = "no-attach")] no_attach: bool` to `Start`.
  - In `run()` match: pass `desc.or(description).as_deref()` to `new::run`; after creation, call `start::run_with_attach(&ctx, &ident, !no_attach)` unless `draft`.
- [ ] Refactor `start` (crates/agency/src/commands/start.rs)
  - Extract `run_with_attach(ctx, ident, attach)` and move existing logic into it; conditionally call `tmux::attach_session`.
  - Make `run` delegate to `run_with_attach(..., true)`.
- [ ] Adapt `new` command (crates/agency/src/commands/new.rs)
  - Change signature to `run(ctx, slug, agent, desc)`.
  - If `desc` provided: set `content.body = desc.trim().to_string()` and write immediately; log creation.
  - Else: keep current TTY-based editor behavior; bail only in editor path when empty.
- [ ] Update tests to remove `--no-edit` (crates/agency/tests/cli.rs)
  - Delete all `--no-edit` arguments in calls to `new` and helper invocations.
  - Ensure direct `cmd.arg("new")` tests (like invalid slug) still work without a description.
- [ ] Make tests non-interactive by default (crates/agency/tests/common/mod.rs)
  - In `TestEnv::new_task`, if `extra_args` doesn’t include `--description`, append `--description` with a fixed value (e.g., `Automated test`). Keep auto-adding `--draft` if missing.
- [ ] Add a test for description persistence (crates/agency/tests/cli.rs)
  - Create a task with `--draft --description "Automated test body"` and assert the markdown contains that text.
- [ ] Optional: add a no-attach test guarded by availability checks.
- [ ] Run `just check`, `just test`, `just check-verbose`, and format (`just fmt` or `just fix`).

## Questions
1) Keep both positional `desc` and `--description`? Default: Yes, positional for convenience; long option for clarity and scripts.
2) Make `--draft` and `--no-attach` conflict? Default: Yes; `--draft` implies no start at all.
3) Accept empty `--description` (after trim) as empty body? Default: Yes, but editor path still bails on empty to signal cancellation.
4) Add `--no-attach` to `start` as well? Default: Yes (you confirmed).
5) OK to change tests by centralizing a default description in `TestEnv::new_task`? Default: Yes; callers can still override by passing their own `--description`.
