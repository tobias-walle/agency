# Agency

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

## Tech Stack

- Rust >=1.89 (workspace uses Edition 2024)
- macOS and Linux supported (Windows not supported)

## Structure

- `./docs/specs/SPEC-[id]-[slug].md` - Store for specifications. They include product and architecture decisions and should be kept up to date with changes.
- `./docs/plans/PLN-[id]-[slug].md` - Concrete plans to add or modify features or to fix bugs. They follow the structure defined in `./docs/specs/SPEC-1-plan-format.md`. Plans always are a snapshot in time, and might not represent the current decision or project structure.
- `./justfile` - Project scripts
- `./crates` - Contains all the rust crates (apps & libraries) used for this project

## Justfile

All common scripts life in `./justfile`.
Prefer using the `just` commands over the direct `cargo` commands.

Available recipes:

- `just setup` - `cargo check`
- `just agency *ARGS` - `cargo run -p agency -- {ARGS}`
- `just test *ARGS` - `cargo nextest run {ARGS}`
- `just check` - `cargo clippy --tests`
- `just fmt` - `cargo fmt --all`
- `just fix` - `cargo clippy --allow-dirty --allow-staged --tests --fix` then `just fmt`

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

## Rules

- Indent code always with 2 spaces
- When committing, follow the conventional commits format
- Prefer ASCII punctuation in docs and code. Avoid long dashes (—) and Unicode arrows (→, ↔); use `-`, `->`, `<->` instead.
- Only add dependencies via `cargo add [pkg]` (exception: dependency already exists). Never modify Cargo.toml directly.
- Make use of subagents via the `task` tool to keep the context concise
- Use the `api-docs-expert` subagent when working with libraries
  - Lookup new APIs before you use them
  - Check correct API use when encountering errors
- Never use single letter variable names if they span more than 3 lines
- You SHOULD use TDD then appropriate:
  - Fixing bugs -> Write tests before implementation
  - Implement new features, with unclear final solution -> Write tests after implementation
- Before writing or editing Rust code, you MUST read `./docs/guides/RUST_BEST_PRACTICES.md` and follow it

## Testing

- Keep tests readable and focused on behavior. Extract common functionality into helpers to keep the tests high signal.
- Highly emphasize actionable assertion output (what, why, actual vs expected).
- Prefer polling with bounded timeouts over fixed sleeps to reduce flakiness.
- Use `git2` for local repositories instead of shelling out to `git`.
- Avoid global env mutations; prefer per-command `.env()` or scoped guards.
