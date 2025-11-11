# PLAN: Slug handling and empty-editor cancellation
Make slug input resilient, surface errors in the TUI, and avoid creating tasks when the editor exits with empty content.

## Goals
- Show an error in the TUI when a slug is invalid instead of silently closing the input
- Slugify the slug input (lowercase, non-alnum collapsed to '-') instead of failing
- If the editor is closed without any input (trimmed), do not create a task and log an error

## Out of scope
- Changing behavior for `--no-edit` (empty body allowed)
- Changing task/session/daemon protocols
- Adding complex transliteration for non-ASCII letters (keep Unicode letters)
- Larger TUI UX redesign beyond error feedback described here

## Current Behavior
- TUI new-task input overlay validates and silently closes on error:
  - crates/agency/src/tui/mod.rs:512-519
    ```rust
    let Ok(slug) = crate::utils::task::normalize_and_validate_slug(&state.slug_input) else {
      state.mode = Mode::List;
      continue;
    };
    ```
  - Result: Nothing is printed in the TUI on invalid slug.
- Slug validation only lowercases and enforces strict rules; does not slugify:
  - crates/agency/src/utils/task.rs:109-128 `normalize_and_validate_slug`
    - Errors for starting with a non-letter and for any non-alnum `-` characters
    - Tests enforce failure for `"1invalid"` and `"bad/slug"` (lines ~335-341)
- Task creation writes the file and logs success before editor interaction:
  - crates/agency/src/commands/new.rs:38-66
    - Calls `write_task_content` and `log_info!("Create task ...")`
    - Then optionally opens editor
- Editor helper returns `None` for empty (trimmed) content but the file already exists:
  - crates/agency/src/utils/task.rs:289-311 `edit_task_description`

## Solution
- Slugify-first then validate:
  - Lowercase input; replace any non-alphanumeric run with a single `-`; trim leading/trailing `-`
  - Validate non-empty after slugify
  - Allow leading digits (no "must start with a letter" rule)
- TUI error feedback on invalid slug:
  - On Enter with invalid slug, keep the overlay open and `log_error!(...)` a clear message
- Defer file creation until after editor confirms content:
  - For interactive flow (no `--no-edit`): open editor first; only write file and log success if content is non-empty
  - If editor closes with empty content: bail with a clear error; no file created, no notify
  - Keep `--no-edit` behavior unchanged
- Tests: update slug validation tests to cover slugify behavior and empty-after-slugify errors

## Architecture
- Modified files
  - crates/agency/src/utils/task.rs
    - Implement slugify-first `normalize_and_validate_slug`
    - Update `normalize_and_validate_slug_rules` tests
  - crates/agency/src/tui/mod.rs
    - In `Mode::InputSlug` Enter handler: on error, `log_error!` and keep `Mode::InputSlug`
  - crates/agency/src/commands/new.rs
    - Reorder creation: editor first; write + log only after non-empty content; bail on empty

## Detailed Plan
- [ ] Implement slugify-first `normalize_and_validate_slug`
  - Lowercase; map non-alnum to `-`; collapse; trim; error if empty
  - Keep Unicode letters/digits allowed via `is_alphanumeric()`
- [ ] Update tests in `utils/task.rs`
  - Success: `"Alpha World" -> "alpha-world"`, `"alpha_world" -> "alpha-world"`, `"alpha---world" -> "alpha-world"`, `"1invalid" -> "1invalid"`
  - Error: `""`, `"---"`, `"   "`, `"**"` -> empty after slugify
- [ ] TUI: show error and keep overlay
  - In `Mode::InputSlug` Enter handler, on `Err`, call `log_error!("New failed: {}", err)` and do not switch to list mode
- [ ] Defer task file creation until after editor content
  - In `commands/new.rs`, for interactive flow: open editor; if `Some(updated)`, write and log; if `None`, `bail!("New canceled: empty description")`
  - Preserve `--no-edit` path as-is
- [ ] Verify and format
  - Run `just check` and fix errors/warnings
  - Run `just fmt`

## Questions
1) Allow leading digits after slugify? Default: Yes, as long as non-empty
2) Keep Unicode alphanumerics without ASCII-folding? Default: Yes
3) Error wording for empty-after-slugify? Default: `Invalid slug: empty after slugify`
4) Keep overlay open on invalid slug and log error? Default: Yes
5) On empty editor, bail and avoid creating any files? Default: Yes
6) Keep `compute_unique_slug` unchanged? Default: Yes
7) Preserve `--no-edit` behavior (still creates file/body immediately)? Default: Yes

