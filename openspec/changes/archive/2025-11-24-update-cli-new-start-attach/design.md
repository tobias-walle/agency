# Design: CLI `agency new` starts and attaches without opening editor

## Architecture Overview

- Files impacted:
  - `crates/agency/src/lib.rs` – CLI subcommand parsing and the `Commands::New` dispatch logic that chains `new::run` and `start::run_with_attach`.
  - `crates/agency/src/commands/new.rs` – Task creation behavior, including when the editor is invoked, how descriptions are handled, and how tasks are persisted.
  - `crates/agency/tests/cli_new.rs` – CLI integration tests covering `agency new` semantics.
  - `crates/agency/tests/common/test_env.rs` and `crates/agency/tests/common/agency.rs` – Test helpers that construct CLI invocations and simulate interactive/non-interactive environments.
- Symbols impacted:
  - `Commands::New` enum variant in `crates/agency/src/lib.rs` and its match arm in `run_command`.
  - `commands::new::run` function in `crates/agency/src/commands/new.rs`.
  - `TestEnv::new_task` and potentially a new helper for "start-and-attach" flows in `crates/agency/tests/common/agency.rs`.

## Current Behavior

- CLI:
  - `Commands::New` in `crates/agency/src/lib.rs` currently:
    - Accepts `slug`, optional positional `desc`, `--agent`, `--draft`, `--description`, and `--no-attach`.
    - Calls `commands::new::run(ctx, &slug, agent.as_deref(), provided_desc.as_deref())` where `provided_desc` is the positional or `--description` flag.
    - If `draft` is false, it then calls `commands::start::run_with_attach(ctx, &ident, !no_attach)?` to start (and by default attach to) the task.
  - `commands::new::run` in `crates/agency/src/commands/new.rs`:
    - Normalizes and validates the slug and computes the next task id.
    - Determines a base branch by inspecting the current git repo; errors if not in a git repo.
    - Builds YAML front matter with optional `agent` and `base_branch` and constructs a `TaskRef` and `TaskContent`.
    - Behavior depends on `desc`:
      - If `desc` is `Some`, it trims and writes the body directly, logs task creation, and never touches the editor.
      - If `desc` is `None` and `stdout` is a TTY, it invokes `edit_task_description(...)` to open the configured editor, only writes the file if the user provides non-empty content, and otherwise bails with `"Empty description"`.
      - If `desc` is `None` and `stdout` is not a TTY, it writes an empty body and logs task creation without opening an editor.
    - Worktree and bootstrap are created lazily at attach time; `new` itself only creates the task file and metadata.
- TUI:
  - `crates/agency/src/tui/mod.rs` uses `crate::commands::new::run` directly in the `Mode::InputSlug` handler and then calls `start::run_with_attach(..., false)` for "New + Start"; editor behavior in this flow relies on `commands::new::run` and the stdout TTY detection.
- Tests:
  - `crates/agency/tests/cli_new.rs` primarily exercises task creation, slug normalization, bootstrap behavior, and agent front matter.
  - `TestEnv::new_task` always adds `--draft` and `--description` unless the caller overrides them, so existing tests avoid editor interaction and default start/attach behavior.

## Target Behavior

- Default CLI behavior:
  - `agency new <slug>` MUST:
    - Create the task immediately without launching an editor.
    - Start the task session.
    - Attach to the task session.
  - The description MAY initially be empty unless provided via positional or `--description` flag.
- Draft behavior:
  - `agency new --draft <slug>` MUST:
    - Create the task as a draft.
    - NOT start the session.
    - NOT attach to the session.
    - In interactive TTY mode, when no positional description and no `--description` flag are provided, open the configured editor to author the initial description (preserving current behavior).
- No-attach behavior:
  - `agency new --no-attach <slug>` MUST:
    - Create the task immediately.
    - Start the task session.
    - NOT attach to the session.
- Description handling:
  - Positional `desc` and `--description` MUST:
    - Pre-populate the task body without opening an editor.
    - Respect `--draft` and `--no-attach` semantics for whether the session is started and/or attached.

## Design Decisions and Trade-offs

- Separation of concerns:
  - Maintain `commands::new::run` as the single place responsible for task file creation and editor invocation, so both CLI and TUI continue to share a common implementation.
  - Adjust CLI dispatch (`Commands::New` in `lib.rs`) to control when descriptions are provided and when editor paths are exercised, instead of inlining new logic directly into `commands::new::run` that might affect the TUI.
- Editor usage:
  - For the default `agency new <slug>` flow, avoid invoking the editor by ensuring a description is always provided (even if empty or a placeholder), so `commands::new::run` takes the non-editor path.
  - Preserve editor-based drafting primarily for `--draft` flows or when invoked from contexts (like the TUI) that explicitly rely on `commands::new::run` with `desc = None` and a TTY.
- Readability and maintainability:
  - Keep `run_command` match arms linear, with clear early decisions based on flag combinations.
  - Avoid adding complex branching or nested conditionals inside `commands::new::run`; instead, introduce small helper functions if the CLI dispatch needs additional behavior.
  - Ensure tests cover the new combinations (`--draft`, `--no-attach`, `--description`) to prevent regressions in future refactors.

## Planned Changes

- `crates/agency/src/lib.rs`
  - Update the `Commands::New` match arm in `run_command` so that:
    - The default case (`draft == false`) uses `commands::new::run` in a way that bypasses editor usage and then calls `start::run_with_attach` with `attach = true` unless `--no-attach` is set.
    - The draft case (`draft == true`) preserves the ability to use the editor (by passing `desc = None` when appropriate) and skips starting/attaching.
    - The interplay between positional description and `--description` remains consistent and linear, with a single `provided_desc` that is passed through.
- `crates/agency/src/commands/new.rs`
  - Keep the core logic for slug validation, base branch detection, front matter, and description handling intact.
  - If necessary for clarity, extract small helpers for detecting interactive vs non-interactive modes, but avoid changing existing behavior relied on by the TUI.
- Tests (`crates/agency/tests`)
  - Extend `cli_new.rs` with new tests that explicitly exercise:
    - `agency new <slug>` starting and attaching without editor involvement.
    - `agency new --draft <slug>` behavior, including editor usage when configured.
    - `agency new --no-attach <slug>` starting without attaching.
    - Description handling via pos/flag in combination with draft/no-attach flags.
  - Consider adding a dedicated helper (e.g., `new_task_started_and_attached`) in `common/agency.rs` for readability, while keeping the tests linear and easy to follow.

## Architecture

- New and modified files/symbols (high level):
  - `crates/agency/src/lib.rs`
    - Modify `run_command` match arm for `Commands::New` to encode the new default behavior and flag interactions.
  - `crates/agency/src/commands/new.rs`
    - Potentially add small helpers (e.g., for description handling) if needed for clarity.
  - `crates/agency/tests/cli_new.rs`
    - Add new integration tests for CLI `new` behavior.
  - `crates/agency/tests/common/agency.rs`
    - Optionally add a helper to capture the start-and-attach behavior of `agency new` in tests.

## Testing Strategy

- Unit / integration coverage:
  - Focus on integration-style tests using `assert_cmd` (existing pattern) to validate CLI behavior end-to-end.
  - Use `TestEnv::run` and `TestEnv::run_tty` to simulate non-interactive and interactive environments as needed, ensuring editor behavior is only exercised deliberately.
- Commands to run after implementation:
  - `just check`
  - `just test`
  - `just check-strict`

# PLAN: Update CLI `agency new` behavior

[Plan for changing `agency new` so it starts and attaches without opening an editor by default.]

## Goals

- Make `agency new <slug>` create, start, and attach without opening an editor.
- Preserve `--draft` as a non-start, non-attach flow.
- Preserve `--no-attach` as a start-without-attach flow.
- Keep TUI behavior stable while sharing `commands::new::run`.

## Out of scope

- Changes to TUI keybindings or overlays.
- Changes to daemon protocol or attach implementation.
- New configuration options for `agency new` behavior.

## Current Behavior

- `crates/agency/src/lib.rs:22-87` defines `Commands::New` and dispatches to `commands::new::run` followed by `start::run_with_attach` when `draft` is false.
- `crates/agency/src/commands/new.rs:15-87` controls slug validation, base branch detection, front matter, description handling, and editor invocation.
- `crates/agency/src/tui/mod.rs:672-715` calls `commands::new::run` and `start::run_with_attach` for TUI "New + Start" behavior.
- `crates/agency/tests/cli_new.rs:1-140` covers task creation, bootstrap behavior, slug rules, and agent front matter, but not start/attach semantics.

## Solution

- Update CLI dispatch for `Commands::New` so the default path never opens an editor and always starts and attaches.
- Continue to route draft flows through `commands::new::run` with `desc = None` when we want to allow editor usage.
- Ensure description flags and positional descriptions always bypass the editor and respect draft/no-attach flags.
- Add CLI tests that assert start and attach behavior for the new combinations.

## Architecture

- `crates/agency/src/lib.rs`
  - `Commands::New` enum variant.
  - `run_command` match arm for `Commands::New`.
- `crates/agency/src/commands/new.rs`
  - `run` function for task creation and editor handling.
- `crates/agency/src/tui/mod.rs`
  - `Mode::InputSlug` handler using `commands::new::run` and `start::run_with_attach`.
- `crates/agency/tests/cli_new.rs`
  - New tests for default, draft, and no-attach flows.
- `crates/agency/tests/common/agency.rs`
  - `TestEnv::new_task` and potential helper for start-and-attach flows.

## Testing

- Integration: CLI tests in `crates/agency/tests/cli_new.rs` covering:
  - `agency new <slug>` default behavior (start + attach, no editor).
  - `agency new --draft <slug>` (no start, no attach).
  - `agency new --no-attach <slug>` (start, no attach).
  - `agency new` with positional and `--description` flags.
- Commands:
  - `just check`
  - `just test`
  - `just check-strict`

## Detailed Plan

1. [ ] 1.1 Extend CLI spec for `agency new` behavior in `openspec/changes/update-cli-new-start-attach/specs/cli-new-behavior/spec.md` if review feedback requires tweaks.
2. [ ] 2.1 Update `Commands::New` dispatch in `crates/agency/src/lib.rs` to implement the new default, draft, and no-attach behavior without editor usage in the default flow.
3. [ ] 3.1 Verify `commands::new::run` behavior remains compatible with TUI usage in `crates/agency/src/tui/mod.rs`, adjusting only if strictly necessary.
4. [ ] 4.1 Add or update integration tests in `crates/agency/tests/cli_new.rs` to cover start/attach behavior and description handling.
5. [ ] 5.1 Run `just check`, `just test`, and `just check-strict` and address any issues.

## Questions

1. Should `agency new --description "..." <slug>` still start and attach by default (current assumption: yes, unless `--draft` or `--no-attach` is set)?
2. Should there remain any CLI path where `agency new` opens the editor directly (current assumption: only via `--draft` or via TUI calling `commands::new::run` with `desc = None`)?
3. Do we need an explicit flag to restore the old "open editor then start+attach" behavior for users who prefer it (current assumption: no, rely on `agency edit` + `agency start` instead)?
4. In non-interactive contexts (e.g., CI scripts), is it acceptable that `agency new <slug>` implicitly attempts to attach, or should scripts be expected to use `--no-attach` (current assumption: scripts should opt into `--no-attach` explicitly)?
