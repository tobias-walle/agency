# PLN-2: add `agency new [slug]` with config and TTY-aware colors

Date: 2025-10-26

Introduce a `new` subcommand that creates task files under `<cwd>/.agency/tasks/[id]-[slug].md` with auto-incrementing ids, unique lowercase slugs (supporting umlauts), centralized config, and TTY-aware colored output using `anstream` + `owo-colors`.

## Goals

- Add `agency new [slug]` subcommand using Clap.
- Create `<cwd>/.agency/tasks` if missing and log its creation once.
- Autoincrement `[id]` starting at 1 based on existing files in the tasks folder.
- Enforce unique `[slug]` and lowercase, allowing Unicode letters (e.g., umlauts), digits, and `-`.
- On success: print `Task [id]-[slug] created ✨` with `[id]-[slug]` in cyan when TTY.
- On duplicate slug: print `Task with slug [slug] already exists` in red (TTY) and exit with code `1`.
- Centralize configuration in `config.rs` with `AgencyConfig::new(cwd)`.
- Centralize stdout/stderr and color gating using `anstream` + `owo-colors`.
- Add a minimal integration test using a temporary directory that only asserts the tasks folder is created.

## Non Goals

- No extra flags (e.g., `--force`, `--dry-run`).
- No color assertions in tests.
- No Windows support.
- No plan to validate file contents beyond the required header line.

## Current Behavior

- `crates/agency/src/lib.rs:5` defines a bare Clap CLI and `run()` stub with no subcommands:
  ```rust
  /// Agency - An AI agent manager and orchestrator in your command line.
  #[derive(Debug, Parser)]
  #[command(author, version, about, long_about = None)]
  pub struct Cli {}

  pub fn run() -> Result<()> {
    let _cli = parse();
    Ok(())
  }
  ```
- `crates/agency/src/main.rs:1` calls `agency::run()`.
- Tests only cover `--help` in `crates/agency/tests/cli_help.rs`.
- `crates/agency/Cargo.toml` currently lacks `anstream` and `owo-colors`.

## Solution

- Extend Clap CLI with a `New` subcommand accepting a single `slug` argument.
- Implement `config.rs` with `AgencyConfig::new(cwd)` returning a config that knows `.agency/tasks` under the given `cwd`.
- Add `term.rs` helpers to print via `anstream::stdout()/stderr()` and rely on `owo-colors` for styling when TTY is supported (colors auto-stripped otherwise).
- Implement `commands::new::run(cfg, slug)` to:
  - Normalize slug to lowercase; validate all chars are Unicode alphanumeric or `-`.
  - Ensure `<cwd>/.agency/tasks` exists; log creation if newly created.
  - Enforce unique slug by checking for `^\d+-[slug]\.md` in tasks dir.
  - Compute next id as `1 + max(id)` over `^\d+-.*\.md` files.
  - Write `<tasks_dir>/<id>-<slug>.md` with content `# Task [id]: [slug]`.
  - Print success: `Task {id}-{slug} created ✨` with the `{id}-{slug}` cyan if TTY.
  - On duplicate slug, print `Task with slug [slug] already exists` in red (TTY) to stderr and exit with code `1`.

## Detailed Plan

- [ ] Add dependencies (via cargo add)
  1. Runtime: `anstream`, `owo-colors`
  2. Dev: `tempfile` for integration test temp dirs

- [ ] Centralized config
  1. File: `crates/agency/src/config.rs`
  2. Add `pub struct AgencyConfig { cwd: PathBuf }`
  3. `impl AgencyConfig { pub fn new(cwd: impl Into<PathBuf>) -> Self; pub fn tasks_dir(&self) -> PathBuf; }`
  4. Hardcode tasks subpath to `.agency/tasks`

- [ ] Terminal helpers
  1. File: `crates/agency/src/term.rs`
  2. Provide `pub fn stdout() -> anstream::Writer`, `pub fn stderr() -> anstream::Writer`
  3. Provide `pub fn print_ok(msg: impl Display)` and `pub fn print_err(msg: impl Display)`

- [ ] CLI wire-up
  1. File: `crates/agency/src/lib.rs`
  2. Add `#[derive(Subcommand)] enum Commands { New { slug: String } }`
  3. Update `Cli` to accept `#[command(subcommand)] command: Option<Commands>`
  4. In `run()`, build `AgencyConfig::new(std::env::current_dir()?)` and dispatch `New` to handler
  5. Add modules: `mod config; mod term; mod commands;`

- [ ] Implement `new` command
  1. Files: `crates/agency/src/commands/mod.rs`, `crates/agency/src/commands/new.rs`
  2. `normalize_and_validate_slug()` -> lowercase + `is_alphanumeric()` or `-`
  3. `slug_exists()` -> check `^\d+-[slug]\.md`
  4. `next_id()` -> scan `^\d+-.*\.md`, parse ids, `max + 1` or `1`
  5. Create dir if missing (log once), write file, print success; on duplicate, print error and exit 1

- [ ] Tests (integration)
  1. File: `crates/agency/tests/common/mod.rs` with `TestEnv` using `tempfile::TempDir`
  2. File: `crates/agency/tests/cli_new.rs` that runs `agency new märchen-test` with `.current_dir(env.path())`
  3. Assert only that `<tmp>/.agency/tasks` directory exists

- [ ] Docs
  1. Update `AGENTS.md` with stdout/stderr best practices using `anstream` + `owo-colors`

- [ ] Verify
  1. `just check`, `just test`
  2. Manual: `just agency new my-task`

## Notes

- Unicode-aware `to_lowercase()` and `char::is_alphanumeric()` allow umlauts and other letters.
- Using `anstream` writers centralizes TTY-aware color handling; `owo-colors` styles are auto-stripped on non-TTY.
- Errors go to stderr; successes to stdout. Tests avoid color checks as they depend on TTY.
