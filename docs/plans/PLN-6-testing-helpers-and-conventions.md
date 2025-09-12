# PLN-6 Testing Helpers and Conventions

## Goal

Introduce shared testing helpers and conventions to reduce duplication, improve readability, and increase test stability across the workspace.

## Outcome

- Centralized helpers in `crates/test-support` for daemon lifecycle, RPC client, CLI wrappers, git repo init, PTY attach/IO, and scoped env guards.
- Consistent test naming and file structure.
- Flakiness reduced via polling with timeouts instead of fixed sleeps.

## Phases

### Phase 1: Foundations (<= 0.5 day)

- Add `test-support` modules: `daemon`, `rpc`, `cli`, `git`, `pty`, `env`.
- Document usage inline with concise examples.
- No behavioral changes; only helper introduction and smoke validation.

### Phase 2: Initial Migration (<= 0.5 day)

- Migrate representative tests in `crates/core` (e.g., `daemon_e2e.rs`, `pty.rs`) to the helpers.
- Normalize naming: feature-oriented files and `subject_action_expected` test names.
- Replace fixed sleeps with polling utilities.

### Phase 3: Broad Adoption (<= 0.5 day)

- Migrate `crates/cli` tests to the helpers (attach flows, daemon lifecycle, init/status).
- Ensure consistent repo init via `git2` and CLI wrappers for env handling.
- Add short best-practices note to `AGENTS.md` (done).

## Notes

- Keep helpers minimal and composable; avoid leaking implementation details.
- Add new Context7 IDs to `AGENTS.md` only if new crates are introduced.
- Prefer per-test isolation; mark serial only when unavoidable.
