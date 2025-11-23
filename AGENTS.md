<!-- OPENSPEC:START -->

# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:

- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:

- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# OpenSpec additional instructions

- In the openspec design, you MUST explain the architecture
  - Mention all files and symbols you want to add, edit, or delete at a high level
  - Explicitly highlight the trade-offs and how you considered readability and maintainability

# Committing

- Use conventional commits e.g.:
  - `docs: add change proposal for ...`
  - `feat: implement ...`
  - `fix: fix ...`
- You MUST add the files and the create the commit in the same command for easy review e.g.:
  - `git add fileA.rs fileB.rs && git commit -m "feat: ..."`
- Keep commit messages short. Never add semicolons in them.
- If you are committing a spec change always start with `docs: add spec change for ...`

# Code Style

- Naming
  - Avoid single-letter variable names except trivial indices; use descriptive names (e.g., `draft_text`, `editor_args`).
  - Avoid similar names in the same scope (e.g., `argv` vs `args`); choose clearly distinct identifiers.
  - Avoid underscore-prefixed bindings in production and tests; use descriptive names.
- Function Size & Structure
  - Keep functions under ~100 lines; extract helpers to satisfy `clippy::too_many_lines`.
  - Reduce nesting: early-returns, guard clauses, and splitting logic into small functions.
  - Prefer `let...else` over manual `match` for input validation and early exits.
  - Do not place item declarations after statements in a scope; declare items before executable code.
  - Prefer `while let` loops for stream/frame reads or "read-until-end" patterns instead of `loop { match ... }`.
- Match Hygiene
  - Merge identical match arms; avoid wildcard `_` on enums where future variants are possible.
  - Use explicit variants or a catch-all with clear handling.
- Documentation
  - For any function returning `Result`, include a `# Errors` section describing failure cases.
- Conversions
  - Avoid lossy numeric casts; use `TryFrom`, checked conversions, or explicit bounds handling.
  - Do not wrap return values in `Result` without actual error paths.
- Parameter Passing
  - Prefer passing by reference when the value is not consumed.
  - Prefer `Option<&T>` over `&Option<T>` for optional borrows.
- Error Handling
  - Do not panic in library code; return errors instead. In tests, prefer `assert!`/`assert_eq!` over `panic!`.
- Lint Hygiene
  - Prefer fixing code over broad `#[allow(...)]`; when needed, keep allows local and narrowly scoped.
- You MUST keep the code linear (avoid nesting)
- If this is not given, you MUST do the following:
  - Detect duplicated code and extract it to seperate functions
  - Detect strong nesting and create functions to reduce it
  - Use language feature to reduce nesting

# Tests

- Integration tests are living in `./crates/agency/tests/`
- You MUST use the TestEnv in `./crates/agency/tests/common/mod.rs` for all tests
- Every integration test needs to be wrapped into `TestEnv::run`
- Readability is extremly important. Tests must be linear. Add new helpers to the TestEnv if it helps readability and avoids complex method chains inside the tests.

# Rules

- You MUST run `just check` regulary to detect compile errors
- You MUST run `just test` `just check-strict` after finishing your. You MUST fix all warnings and errors.
- You MUST use `cargo add` to add dependencies
- If you remove code, you MUST NEVER replace it with useless comments (Like `// removed ...`, `// deleted ...`, etc.). If you find comments like this always delete them.
- TTY-dependent tests (e.g. interactive TUI tests) are marked with `#[ignore = "needs-tty"]` and MUST be run via `just test-tty` on a real terminal outside the sandbox.
