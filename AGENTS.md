# Agency

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

## Tech Stack

- Rust >=1.89 (workspace uses Edition 2024)
- macOS and Linux supported (Windows not supported)

## Structure

- `./docs/plans/PLN-[id]-[slug].md` - Concrete plans to add or modify features or to fix bugs.
- `./docs/rules/[slug].md` - Special rules for you, the AI Agent, to read if needed. See also [Conditional Rules](#conditional-rules).
- `./justfile` - Project scripts
- `./crates` - Contains all the rust crates (apps & libraries) used for this project
- `./crates/agency/src/` - Source files for the CLI app
  - `commands/` - One file per CLI command (entrypoints)
  - `utils/` - Shared helpers grouped by topic (e.g. git, task, term)
  - `config.rs` - Single source of truth for all kinds of configuration for agency
  - `lib.rs`
  - `main.rs`
- `./crates/agency/tests/` - Tests for the CLI app
  - `common/`
    - `mod.rs` - Common test helpers
  - `cli.rs` - Integration tests for the cli apps
- `./crates/agency/defaults/agency.toml` - Built-in default configuration for agents

## Justfile

- All common scripts live in `./justfile`.
- Prefer using the `just` commands over the direct `cargo` commands.
- Most important commands:
  - `just agency ...` - Runs the app with the given commands
  - `just test ...` - Runs the tests with `nextest run`
  - `just check` - Check the code for errors. Use this often and fix the errors immediately.
  - `just fix` - Fixes all linting errors and prints pedantic warnings and formats the code afterwards. Use this if the user asks you to fix the lints. In this case make sure all warnings are resolved if they couldn't be fixed automatically.
  - `just fmt` - Just formats the code. Run this after finishing a feature or fix.

## Commit Rules

When committing, always follow these rules

- Follow Conventional Commits (e.g. `feat: add new feature`)
- Keep most commits in a single line. Only use the body if there are unexpected changes in the commit.

- Summarize the changes into a single sentence, starting with a lowercase verb.
- The sentence should cover why the changes were made.
- NEVER add semicolons in the message and keep the title shorter than 80 chars.
- Don't add a commit body or footer
- You might want to create multiple commits if the changes are not related.
- After every task, you SHOULD:
  - Run `just test` and make sure it runs
  - Run `just fix` and fix all warnings
  - Commit the changes

Add the files and commit in a single command, e.g. `git add file1.ts file2.ts && git commit -m "..."`

## Dependencies

- You MUST add dependencies via `cargo add [pkg]` -> Never modify Cargo.toml directly.
- You SHOULD use the `Context7` MCP when working with libraries
  - Lookup new APIs before you use them
  - Check correct API use when encountering errors

## Testing

- Run tests with `just test`. Don't pass `-q`, it will fail.
- You MUST put unit tests into the same file as this is a Rust best practice
- Keep tests readable and focused on behavior. Extract common functionality into helpers to keep the tests high signal.
- Highly emphasize actionable assertion output (what, why, actual vs expected).
- Prefer polling with bounded timeouts over fixed sleeps to reduce flakiness.
- Use `gix` for local repositories instead of shelling out to `git` (exception, `gix` isn't supporting the functionality).
- Avoid global env mutations; prefer per-command `.env()` or scoped guards.
- For tests that need environment variables, use the `temp-env` crate (closure APIs like `with_var`/`with_vars`) to set/unset variables temporarily and restore them automatically.
- You SHOULD use TDD when appropriate:
  - Fixing bugs -> Write tests before implementation
  - Implement new features, with unclear final solution -> Write tests after implementation

### CLI Test Practices

- Use `assert_cmd::Command::cargo_bin("agency")` for CLI tests.
- Chain calls: `.arg(...).write_stdin(...).assert().success()`.
- Prefer pipes over PTY; only use PTY when testing PTY.
- Write files under `<crate>/target/test-tmp` via `common::tmp_root()`.
- Override XDG paths or sockets with `.env(...)` to the test workdir.
- Keep output assertions minimal: prefer `contains(...)` over full-output diffs.
- Verify a specific log/message in one focused test; avoid repeating it across tests.

## Code Style

- Indent code always with 2 spaces
- Prefer ASCII punctuation in docs and code. Avoid long dashes (—), semicolons (;) and Unicode arrows (→, ↔); use `-`, `->`, `<->` instead.
- Do not use single letter variables, as they are hard to understand
- Favor readability, even if it is sometimes a bit more verbose. Avoid heavy nesting.
- After you finish all your tasks
  - You MUST run `just check` and fix all warnings & errors
  - Afterwards you MUST run `cargo fmt` to format the code correctly
- You MUST collapse if statements. Only use nested ifs not not collapsing is not possible.

  ```rust
  // ❌ BAD - Nested if statements
  if event::poll(Duration::from_millis(150))? {
    if let Event::Key(key) = event::read()? {
      // ...
    }
  }

  // ✅ GOOD - Use &&
  if event::poll(Duration::from_millis(150))?
    && let Event::Key(key) = event::read()? {
    // ...
  }

  ```

## Terminal IO

- You MUST use `bail!` for errors, if they should crash the program. They are automatically printed to stderr in red.
- In the CLI/TUI
  - You MUST use the our log macros `log_info!`, `log_success!`, `log_warn!`, `log_error!` (use `crate::log_info`, etc.)
  - Logging Style
    - Info: neutral line; highlight tokens via `utils::log::t`.
    - Success: entire line green; no token highlights.
    - Warn: entire line yellow; no token highlights.
    - Error: entire line red; no token highlights.
    - Tokens (`use utils::log::t`): `t::id`, `t::count` (blue), `t::slug` (bold), `t::branch` (magenta), `t::path`/`t::sha` (cyan).
  - Wording: Uppercase start; verb-first; past tense for confirmations; ASCII; no trailing period; use `->`.
  - Examples: `log_info!("Create task {} (id {})", t::slug(slug), t::id(id));` · `log_success!("Fast-forward {} to {} at {}", base, branch, sha);` · `log_warn!("Clean up: worktree, branch, file");` · `log_error!("Daemon error: {}", msg);`
- In the Daemon
  - You MUST use the log crate for logging (e.g. `log::info!(...)`)

## Async

- You MUST NEVER use `Tokio`. We want to keep the code simple and prefer the use of threads.

## Concurrency

- Use `parking_lot::Mutex` for all new and refactored mutexes; prefer it over `std::sync::Mutex`.
- Access mutex-protected data via centralized helpers to ensure short-lived lock scopes and avoid nested locking.
- Helper naming must reflect intent:
  - `read_*` for helpers that acquire a lock and read/derive data (return owned copies; do not send/IO while locked).
  - `write_*` for helpers that acquire a lock and mutate state (perform I/O or sends only after the lock is released).
- Never send frames or perform blocking I/O while holding a lock; copy out data first, then release the lock and send.
- Keep public/high-level lifecycle methods at the top of files and place general lock helpers at the bottom for clarity.

## Plans

Every time a prompt mentions `PLAN` you must enter the plan mode. In the plan mode, you never write any files (except markdown plans if explicitly requested).

General Workflow in Plan Mode:

1. Gather relevant information.
2. Ask clarifying questions to check all your assumptions. Give each question a number (for easy reference) and a recommended/default answer.
3. After that, start with the full research for the plan.
   The goal of the research is to read all necessary files to get a full picture how the change can be implemented.
   You MUST make sure you got all the relevant facts before generating the final plan.
   The plan should not contain sections like "Read file x to confirm strategy" as you should already know the content of all relevant files before creating the plan.
4. After you have all the required information, finalize everything and present the very concrete, but high level plan to the user

Goal of the process is to discuss about the implementation on a high level, before you already update a lot of files which are hard to revert.

Structure your final plan into the following sections (replace placeholders in `[]`). Add a new line between each section:

- `# PLAN: [title]`
- `[short sentence what this plan is about]`
- `## Goals`
- `[goals (as a bullet point list)]`
- `## Out of scope`
- `[non goals (as a bullet point list)]`
- `## Current Behavior`
- `[how does the system currently work (based on your research). Make sure to directly reference relevant files and code snippets.]`
- `## Solution`
- `[how will the behavior be changed to solve the problem (in bullet points). Stay high level and focus on architecture and avoid verbose Implementation details.]`
- `## Architecture`
- `[overview of the new, modified and deleted files and symbols in a tree of bullet points. Should be more detailed then solution for a quick review of a Tech Lead.]`
- `## Testing`
- `[Bullet point list with the test cases you want to create or modify. Mark each test with "Unit", "Integration" or "E2E".]`
- `## Detailed Plan`
- `HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)`
- `[Numbered, Step by step plan on how to implement the solution. Mention relevant files and code changes in relatively high detail. Make sure the order makes sense. Keep Testing and TDD in mind and always start with tests. Add empty mardown checkboxes '[ ]' before each step for later update.]`
- `## Questions`
- `[Numbered questions and the assumed answers that went into this plan. The user might want to modify the assumptions.]`

Strictly follow the format for plans.

The plan mode ends once the user explicitly tells you to implement/execute the plan.
Then, when implementing a plan, use your TODO list tool to track the progress.

## General Workflow

## Conditional Rules

- Not all rules are included in the `AGENTS.md` file (this file). Some rules are only relevant in specific scenarios.
- You MUST read them before doing anything else, once they are becoming relevant for your task.
- You MUST only read them once. If they are already in your context, don't read them again.

In the following these conditional rule files are listed:

- `./docs/rules/rust-best-practices.md` - You MUST read this if you are working with Rust Code. Either if you are implementing rust code or planning to modify it.

```

```
