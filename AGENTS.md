# Agency

The Agency tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

## Tech Stack

- Rust >=1.89

## Structure

- `./docs/prd/PRD-[id]-[slug].md` - Store for PRD (Product Requirement Documents). Each PRD has an ID and a slug. Then asked to create a PRD, increment the id.
- `./docs/adr/ADR-[id]-[slug].md` - Store the ADR (Architecture Decision Records). Also increment the ids here.
- `./docs/plans/PLN-[id]-[slug].md` - High level plans are stored here. Each plan has one or more phases. Each phase is self contained and should not take more than half a day to build by a skilled engineer.
- `./justfile` - Project scripts

## Justfile

All common scripts should be kept in the `./justfile` for easy access. Update this file if necessary.

Available recipes:

- `just check` # Check for compiler or linting error
- `just agency *ARGS` # Start the app with the given args
- `just test *ARGS` # Run the tests (alias to cargo test)

## Context7 Library IDs

Always look up APIs before you use them and verify usage against the official docs.
Delegate these research tasks to the `api-docs-expert` agent. Give them all the relevant Context7 ids defined below.
If you add a new dependency, resolve its Context7 ID and append it [here](./AGENTS.md).

- chrono -> /chronotope/chrono
- dirs -> /dirs-dev/dirs-rs
- regex -> /rust-lang/regex
- serde -> /serde-rs/serde
- serde_yaml -> /dtolnay/serde-yaml
- thiserror -> /dtolnay/thiserror
- toml -> /toml-rs/toml
- tracing -> /tokio-rs/tracing
- tracing-appender -> /tokio-rs/tracing (subcrate)
- tracing-subscriber -> /tokio-rs/tracing (subcrate)
- clap -> /clap-rs/clap
- git2 -> /rust-lang/git2-rs
- tempfile -> /Stebalien/tempfile
- assert_cmd -> /assert-rs/assert_cmd
- pretty_assertions -> /colin-kiegel/rust-pretty-assertions
- proptest -> /proptest-rs/proptest
- serde_json -> /serde-rs/json
- bytes -> /tokio-rs/bytes
- http-body-util -> /hyperium/http-body (subcrate)
- hyper -> /hyperium/hyper
- hyper-util -> /hyperium/hyper-util
- hyperlocal -> /softprops/hyperlocal
- tokio -> /tokio-rs/tokio
- jsonrpsee -> /paritytech/jsonrpsee
- crossterm -> /crossterm-rs/crossterm

## Rules

- Indent code always with 2 spaces
- Then committing, follow the **conventional commits** format
- Only add dependencies in their version with `cargo add [pkg]` (Exception the dependency already exists in the repo).
  Never modify the Cargo.toml directly.
- Make use of subagents via the `task` tool to keep the context concise
- Use the `api-docs-expert` subagent then working with libraries
  - Lookup new APIs before you use them
  - Check correct API use then encountering an error
- Never use single letter variable names if they span more than 3 lines
