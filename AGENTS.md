# Orchestra

The Orchestra tool orchestrates parallel-running AI CLI agents in isolated Git worktrees.

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
- `just start` # Start the app
- `just test ARGS` # Run the tests

## Rules

- Indent code always with 2 spaces
- Then committing, follow the [conventional commits](https://www.conventionalcommits.org) format
- Only add dependencies in their latest version with `cargo add [pkg]`. Never modify the Cargo.toml directly.
- Make heavy use of the Context7 MCP then working with libraries
  - Lookup new APIs before you use them
  - Check correct API use then encountering an error
