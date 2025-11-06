# Agency

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

## Tech Stack

- Rust >=1.89 (workspace uses Edition 2024)
- macOS and Linux supported (Windows not supported)

## Structure

- `./docs/specs/SPEC-[id]-[slug].md` - Store for specifications. They include product and architecture decisions and should be kept up to date with changes.
- `./docs/plans/PLN-[id]-[slug].md` - Concrete plans to add or modify features or to fix bugs.
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

- You MUST add dependencies via `cargo add [pkg]` -> Never modify Cargo.toml directly.
- You SHOULD use the `Contex7` mcp when working with libraries
  - Lookup new APIs before you use them
  - Check correct API use when encountering errors

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
- Favor readability, even if it is sometimes a bit more verbose. Avoid heavy nesting.
- After you finished all your tasks
  - You MUST run `just check` and fix all warnings & errors
  - Afterwards you MUST run `cargo fmt` to format the code correctly

## Terminal IO

- Use `println!` and `eprintln!` from `anstream` for stdout/stderr to ensure TTY-aware behavior.
- Always use the macro via the alias (`use anstream::println` and/or `use anstream::eprintln`)
- Apply styles with `owo-colors::OwoColorize` and avoid asserting colors in tests as they depend on TTY.
- You MUST use `bail!` for errors, if the should crash the program. They are automatically printed to stderr in red.
- Make userfacing logs colorful. The user should get a modern feel then using our app.

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

## Modes

### Plan Mode

Everytime a prompt starts with `PLAN:` you must enter the plan mode. In the plan mode, you never write any files (except markdown plans if explicitly requested).

General Workflow in Plan Mode:

1. If there is ambiguity, ask clarifying questions. Give each question a number (for easy reference) and a recommended/default answer. Do this before starting the agentic workflow!
2. After all questions were answered or the user ask you too, start with the research for the plan.
   The goal of the research is to read all necessary files to get a full picture how the change can be implemented.
   You MUST make sure you got all the relevant facts before generating the final plan.
   The plan should not contain sections like "Read file x to confirm strategy" as you should already know the content of all relevant files before creating the plan.
3. After you have all the required information, finalize everything and present the very concrete, but high level plan to the user

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
- `## Detailed Plan`
- `HINT: Update checkboxes during the implementation and add short implementation notes (including problems that occurred on how they were solved)`
- `[Numbered, Step by step plan on how to implement the solution. Mention relevant files and code changes in relatively high detail. Make sure the order makes sense. Keep Testing and TDD in mind and always start with tests. Add empty mardown checkboxes '[ ]' before each step for later update.]`
- `## Questions`
- `[Numbered questions and the assumed answers that went into this plan. The user might want to modify the assumptions.]`

Strictly follow the format for plans. Don't read old plans to get the format, as the format changes over time.

The plan mode ends once the user explicitly tells you to implement the plan.
Then implementing a plan use your TODO list tool to track the progress.

### Build Mode

Everytime a prompt starts with `BUILD:` you must enter the build mode. 
In the build mode, you fully focus on execution. This might be a plan or a direct task.

Start execution by managing your TODO list and then continue working on them until you are finished with all tasks.

## Conditional Rules

- Not all rules are included in the `AGENTS.md` file (this file). Some rules are only relevant in specific scenarios.
- You MUST read them before doing anything else, once they are becoming relevant for your task.
- You MUST only read them once. If they are already in your context, don't read the again.

In the following these conditional rule files are listed:

- `./docs/rules/rust-best-practices.md` - You MUST read this if you are working with Rust Code. Either if you are implementing rust code or planning to modify it.
- `./docs/rules/plan-format.md` - You MUST read this then creating plans to understand the best practices.
