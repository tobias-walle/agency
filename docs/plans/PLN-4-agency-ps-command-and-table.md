# PLN-4: add ps command and table helper

Date: 2025-10-26

Add a new `ps` CLI command to list tasks and a reusable terminal table helper.

## Goals

- Add a `ps` subcommand listing tasks by `ID` and `SLUG`.
- Sort tasks ascending by `ID`.
- Print a colored header and rows even when there are no tasks.
- Provide a generic table helper in `utils::term` for future reuse.

## Non Goals

- Display additional columns like status, branch, or worktree path.
- Introduce third-party table libraries or dependencies.
- Assert colors in tests; focus on non-TTY textual content.

## Current Behavior

- CLI subcommands (`new`, `path`, `branch`, `rm`) are defined and wired in `crates/agency/src/lib.rs`.
- Tasks are represented by files `.agency/tasks/{ID}-{slug}.md` and parsed with `TaskRef::from_task_file()` in `crates/agency/src/utils/task.rs`.
- `new` creates the task file, a branch `agency/{ID}-{slug}`, and a worktree under `.agency/worktrees` (`crates/agency/src/commands/new.rs`).
- Terminal output uses `anstream::println!` with optional `owo-colors` styling; tests avoid asserting colors and run in non-TTY (`crates/agency/tests/cli.rs`).

## Solution

- Add a new `Ps` variant to `Commands` in `crates/agency/src/lib.rs` and wire it to `commands::ps::run(&cfg)`.
- Implement `commands/ps.rs` that:
  - Enumerates tasks via a new `list_tasks(cfg)` helper.
  - Sorts tasks ascending by `id`.
  - Renders a two-column table `ID` and `SLUG` using the term helper.
- Extend `utils::task.rs` with `list_tasks(cfg: &AgencyConfig) -> Result<Vec<TaskRef>>` that collects valid tasks or returns an empty list if `.agency/tasks` is missing.
- Add a generic table printer in `utils::term.rs` (e.g., `print_table(headers, rows)`) that:
  - Computes column widths based on headers and rows.
  - Colors headers (e.g., `cyan()`), prints body plain.
  - Right-aligns numeric columns (for `ID`) and left-aligns others.

## Detailed Plan

- [ ] Add tests in `crates/agency/tests/cli.rs`:
  1. `ps_lists_id_and_slug_in_order`: create two tasks; run `agency ps`; assert header and rows `1 alpha-task`, `2 beta-task` in ascending order.
  2. `ps_handles_empty_state`: with no tasks; run `agency ps`; assert only header line is printed.
- [ ] Wire CLI in `crates/agency/src/lib.rs`:
  - Add `Ps` to `Commands` and handle it in `run()`.
- [ ] Export the command in `crates/agency/src/commands/mod.rs`:
  - Add `pub mod ps;`.
- [ ] Implement enumeration in `crates/agency/src/utils/task.rs`:
  - `pub fn list_tasks(cfg: &AgencyConfig) -> Result<Vec<TaskRef>>`.
- [ ] Implement table helper in `crates/agency/src/utils/term.rs`:
  - `pub fn print_table(headers: &[impl std::fmt::Display], rows: &[Vec<String>])`.
- [ ] Implement command logic in `crates/agency/src/commands/ps.rs`:
  - Sort tasks; build rows; call `print_table(&["ID".cyan(), "SLUG".cyan()], &rows)`.
- [ ] Run formatting and checks:
  - `just fmt`, `just check`, `just test`.

## Notes

- The requested ADR path `./docs/adr/ADR-2-plan-format.md` is not present; plan format follows `./docs/rules/plan-format.md`.
- Future enhancements can extend the table with additional columns without changing the helper API.
