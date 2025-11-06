# Agency

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

## Tech Stack

- Rust >=1.89 (workspace uses Edition 2024)
- macOS and Linux supported (Windows not supported)

## Structure

- `./docs/specs/SPEC-[id]-[slug].md` - Store for specifications. They include product and architecture decisions and should be kept up to date with changes.
- `./docs/plans/PLN-[id]-[slug].md` - Concrete plans to add or modify features or to fix bugs. They follow the structure defined in `./docs/rules/plan-format.md`. Plans always are a snapshot in time, and might not represent the current decision or project structure.
- `./docs/rules/[slug].md` - Special rules for you, the AI Agent, to read if needed. See also [Conditional Rules](#conditional-rules).
- `./justfile` - Project scripts
- `./crates` - Contains all the rust crates (apps & libraries) used for this project
- `./crates/agency/src/` - Source files for the CLI app
  - `commands/` - Contains all commands of agency (each command one file)
    - `new.rs`
    - `path.rs`
    - ...
  - `utils/` - General utilities that are used throughout the app. Grouped by topic.
    - `git.rs`
    - `task.rs`
    - `term.rs`
    - ...
  - `config.rs` - Single source of truth for all kinds of configuration for agency
  - `lib.rs`
  - `main.rs`
- `./crates/agency/tests/` - Tests for the CLI app
  - `common/`
    - `mod.rs` - Common test helpers
  - `cli.rs` - Integration tests for the cli apps

## Justfile

- All common scripts life in `./justfile`.
- You MUST read it at the beginning of a chat
- Prefer using the `just` commands over the direct `cargo` commands.

## Commit Rules

Then committing, always follow these rules

- Follow Conventional Commits (e.g. `feat: add new feature`)
- Keep most commits in a single line. Only use the body if there are unexpected changes in the commit.

- Summarize the changes into a single sentence, starting with a lowercase verb.
- The sentence should cover why the changes were made.
- NEVER add semicolons in the message and keep the title shorter than 80 chars.
- Don't add a commit body or footer
- You might want to create multiple commits if the changes are not related.

Add the files and commit in a single command, e.g. `git add file1.ts file2.ts && git commit -m "..."`

## Code Formatting

- Indent code always with 2 spaces
- Prefer ASCII punctuation in docs and code. Avoid long dashes (—), semicolons (;) and Unicode arrows (→, ↔); use `-`, `->`, `<->` instead.
- Never use single letter variable names if they span more than 3 lines

## Dependencies

Currently installed crates:

**Runtime:**

- anstream
- anyhow
- clap (features: derive)
- git2
- owo-colors
- regex

**Dev:**

- assert_cmd
- expectrl
- predicates
- tempfile

- You MUST add dependencies via `cargo add [pkg]` -> Never modify Cargo.toml directly.
- You SHOULD use the `api-docs-expert` subagent when working with libraries
  - Lookup new APIs before you use them
  - Check correct API use when encountering errors
- You SHOULD use the `api-docs-expert` instead of the Context7 directly, even if the user tells you to use Context7/C7

## Testing

- Run tests with `just test`. Don't pass `-q`, it will fail.
- Keep tests readable and focused on behavior. Extract common functionality into helpers to keep the tests high signal.
- Highly emphasize actionable assertion output (what, why, actual vs expected).
- Prefer polling with bounded timeouts over fixed sleeps to reduce flakiness.
- Use `git2` for local repositories instead of shelling out to `git`.
- Avoid global env mutations; prefer per-command `.env()` or scoped guards.
- For tests that need environment variables, use the `temp-env` crate (closure APIs like `with_var`/`with_vars`) to set/unset variables temporarily and restore them automatically.
- You SHOULD use TDD then appropriate:
  - Fixing bugs -> Write tests before implementation
  - Implement new features, with unclear final solution -> Write tests after implementation

## Code Style

- Do not use single letter variables, as they are hard to understand
- After you finished all your tasks
  - You MUST run `just check` and fix all warnings & errors
  - Afterwards you MUST run `cargo fmt` to format the code correctly

## Terminal IO

- Use `println!` and `eprintln!` from `anstream` for stdout/stderr to ensure TTY-aware behavior.
- Always use the macro via the alias (`use anstream::println` and/or `use anstream::eprintln`)
- Apply styles with `owo-colors::OwoColorize` and avoid asserting colors in tests as they depend on TTY.
- You MUST use `bail!` for errors, if the should crash the program. They are automatically printed to stderr in red.

Example:

```rust
use anstream::println;
use owo_colors::OwoColorize as _;

// Foreground colors
println!("My number is {:#x}!", 10.green());
// Background colors
println!("My number is not {}!", 4.on_red());
```

## Async

- You MUST NEVER use `Tokio`. We want to keep the code simple and prefer the use of threads.

## Conditional Rules

- Not all rules are included in the `AGENTS.md` file (this file). Some rules are only relevant in specific scenarios.
- You MUST read them before doing anything else, once they are becoming relevant for your task.
- You MUST only read them once. If they are already in your context, don't read the again.

In the following these conditional rule files are listed:
