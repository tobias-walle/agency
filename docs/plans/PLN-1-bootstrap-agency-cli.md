# PLN-1: bootstrap agency CLI crate with help tests

Date: 2025-10-26

Create a new `crates/agency` CLI crate using Clap derive with only `--help` and add highly readable tests using `assert_cmd` and `expectrl`.

## Goals

- Add `crates/agency` CLI crate that compiles and runs via `just agency`.
- Implement minimal Clap CLI using derive that supports `-h/--help` and `-V/--version`.
- Add a single test file `tests/cli_help.rs` containing both `assert_cmd` and `expectrl` test cases focusing on readable substring assertions.
- Keep `main.rs` thin and place the core CLI definitions in `lib.rs`.

## Non Goals

- No subcommands or business logic yet.
- No interactive flows beyond a basic `expectrl` help check.
- No Windows support.
- No configuration or plugin system yet.

## Current Behavior

- The workspace was slimmed manually to valid members as per your note.
- There is no `crates/agency` crate yet.
- `just agency` expects a package named `agency`.

References:

- `justfile` agency recipe:
```make
agency *ARGS:
  cargo run -p agency -- {{ARGS}}
```

- Plan format to follow:
`docs/specs/SPEC-1-plan-format.md` defines the sections, bullet points, and numbered checklist structure to use.

## Solution

- Create `crates/agency` and structure it for scale:
  - `src/lib.rs` defines a `Cli` with `#[derive(Parser)]` and `#[command(author, version, about, long_about = None)]`.
  - `src/main.rs` calls into `lib` (thin wrapper), e.g. parse and return, no side effects yet.
- Use Clap derive (`features = ["derive"]`) for simplicity and future scalability.
- Add dev-dependencies for testing: `assert_cmd`, `predicates`, and `expectrl`.
- Write `crates/agency/tests/cli_help.rs` with two tests:
  - `assert_cmd` test: run `agency --help`, assert success and check substrings (`Usage`, `Options`, `-h, --help`, `-V, --version`) using `predicates` with `.from_utf8().trim()`.
  - `expectrl` test: spawn the binary attached to a PTY (`Session::spawn(std::process::Command)`), set a short expect timeout, and match a couple of stable anchors (e.g. `Usage`) to validate PTY behavior without over-specifying output.

## Detailed Plan

- [ ] Create crate skeleton for `agency`
  - Files: `crates/agency/Cargo.toml`, `crates/agency/src/lib.rs`, `crates/agency/src/main.rs`
  - `Cargo.toml`:
    - `name = "agency"`, `edition = "2024"`
    - `dependencies`: `clap = { version = "4.5", features = ["derive"] }`
    - `dev-dependencies`: `assert_cmd`, `predicates`, `expectrl`
  - `src/lib.rs`:
    - Define `pub struct Cli` with `#[derive(Parser)]`
    - Add `#[command(author, version, about, long_about = None)]` to auto-fill metadata from Cargo
    - Expose a `pub fn parse() -> Cli` or `pub fn run()` as the primary entry-point (for now, parsing only)
  - `src/main.rs`:
    - Thin wrapper calling into `lib` (e.g. `let _ = agency::Cli::parse();` or `agency::run()?`)

- [ ] Add tests first (readability-focused)
  - Files: `crates/agency/tests/cli_help.rs`
  - Test 1 (`assert_cmd`):
    - Use `assert_cmd::Command::cargo_bin("agency")?` with `--help`
    - Assert `.success()`
    - Assert core substrings with `predicates::str::contains("Usage").from_utf8().trim()` and similar for `Options`, `--help`, `--version`
  - Test 2 (`expectrl`):
    - Construct a `std::process::Command` for the built binary (via `assert_cmd::prelude::CommandCargoExt::cargo_bin("agency")`)
    - Spawn with `expectrl::session::Session::spawn(cmd)`
    - Set a short expect timeout
    - `expect` a stable anchor like `Usage`, then `Eof`
    - Keep assertions minimal and readable

- [ ] Implement minimal Clap derive in `lib.rs`
  - Files: `crates/agency/src/lib.rs`
  - Ensure `version` is set so `-V/--version` appears in help
  - No subcommands or args for now
  - Keep `lib.rs` clean so it’s easy to add modules like `cli/` and subcommand enums later

- [ ] Verify build and tests
  - Commands (via existing recipes):
    - `just setup`
    - `just test`
  - Confirm `just agency -- --help` prints help as expected

- [ ] Prepare structure for scaling (no code yet)
  - Consider future layout: `src/cli/mod.rs` with `#[derive(Subcommand)]` when features grow
  - Plan to keep handler logic separate from argument definitions

## Notes

- C7 research confirms Clap derive as the recommended default for scalable CLIs, using `author`, `version`, `about` pulled from Cargo, and adding `-h/--help` automatically.
- `assert_cmd` with `predicates` is ideal for non-interactive checks; use `.from_utf8().trim()` to avoid whitespace brittleness.
- `expectrl` is reserved for interactive or PTY-sensitive behavior; we include a basic help test here to establish the pattern without over-asserting.
- Keep tests intent-focused and avoid exact full matches to minimize flakiness and verbosity.
- We’ll keep `justfile` unchanged; `just agency` will work once the crate exists.
