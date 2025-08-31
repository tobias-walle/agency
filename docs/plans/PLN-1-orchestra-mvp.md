# PLAN-1: Orchestra MVP Implementation Plan (v1)

This plan breaks the MVP into small, self-contained phases that compile and run after each step, emphasizing fast feedback and early automated testing.

## Workflow

For each phase do the following

- Read the relevant documents to understand the context of this PRD and the architecture:
  - [PRD-1-orchestra-v1.md](../prd/PRD-1-orchestra-v1.md)
  - [ADR-1-mvp.md](../adr/ADR-1-mvp.md)
- Implement the phase. Make heavy use of Context7 to get up to date info on libraries.
- Make sure `just check` is not failing and all tests pass
- Update this document by checking the checkmark in the phase header.
- Also document (very consisly:
  - Problems and how you resolved them
  - Derivations from the plan
  - NEVER document expected changes

## [x] Phase 1: Bootstrap workspace and binary entrypoint

- What to do: Convert to ADR workspace layout with `crates/core`, `crates/cli`, `crates/mcp`, and `apps/orchestra`. Keep all crates compiling with minimal code paths. Route `apps/orchestra/src/main.rs` to call into `cli::run()`.
- Testing strategy: Add CI-friendly smoke test in `apps/orchestra` that runs `--help` and asserts non-error exit. Create `just check` and `just test` cadence. Minimal unit test in `cli` to verify argument parsing.
- Feedback loop: `just start` prints help banner; `just check` passes.

Notes:

- Problems: Binary discovery in smoke test failed due to `CARGO_BIN_EXE_orchestra` not being set when invoking `cargo test -p orchestra`. Resolved by using `assert_cmd::Command::cargo_bin("orchestra")` which compiles and locates the binary under test.
- Derivations: Renamed `core` crate to `orchestra-core` to avoid collision with Rust's standard library name.

## [x] Phase 2: Establish test infrastructure early

- What to do: Set up workspace-level testing scaffolding. Add a Git repo temp utility for tests, a snapshot testing baseline for CLI, and a temp `.orchestra` folder helper. Introduce fake filesystem/log helpers.
- Testing strategy: Introduce `tests/common` module with helpers available via `dev-dependencies`. Add one golden snapshot for `daemon status` placeholder and one for `cli --help`. Configure test-only env vars for deterministic output.
- Feedback loop: `cargo test` runs at workspace and crate levels; snapshots pass.

Notes:

- Problems: `git2` pulled in `openssl` on macOS and failed to compile. Resolved by installing OpenSSL in the environment. Kept default features.
- Derivations: Replaced snapshot testing with normal multiline string assertions and filtered help output to stable lines to avoid churn across clap versions. Added a minimal placeholder `daemon status` implementation printing `daemon: stopped` for determinism.

## [x] Phase 3: Core domain seed (Tasks, Status, YAML front matter)

- What to do: Add pure domain: `TaskId`, `Task`, `Status`, YAML front matter parsing/serialization and transition guards (no IO). Include unit tests and proptests for transitions.
- Testing strategy: Unit tests for serde round-trips, invalid transitions, and status invariants. Proptests if feasible for id/slug parsing.
- Feedback loop: `cargo test -p core` green.

Notes:

- Problems: Clippy flagged match-like patterns; replaced with `matches!` to satisfy `-D warnings`.
- Problems: YAML front matter parsing initially inserted an extra leading newline; trimmed correctly to make round-trip stable.
- Derivations: None.

## [x] Phase 4: Config and filesystem layout utilities

- What to do: Implement `Config` merge (global + project), platform socket defaults, `.orchestra` paths (logs, tasks, worktrees). No daemon yet.
- Testing strategy: Unit tests for default values and merge precedence; tests for platform path derivation guarded with cfgs; tempdir-based tests for `.orchestra` layout.
- Feedback loop: `cargo test -p core` green.

Notes:

- Problems: Clippy flagged nested ifs and manual Default impl; resolved by using `if let` chains and `#[derive(Default)]` with `#[default]` variant.
- Problems: Mutating env vars in tests is unsafe in this toolchain; refactored to a pure function `resolve_socket_path_from()` so tests pass without touching process env.
- Derivations: None.

## [x] Phase 5: Structured logging plumbing

- What to do: Wire `tracing` JSON logs to `./.orchestra/logs.jsonl`. Provide `logging::init(&Config)` and attach `task_id`, `session_id` spans when available.
- Testing strategy: Tempdir tests that initialize logging and assert a JSON line is written with expected fields; use deterministic time via injected clock trait if needed.
- Feedback loop: Running any command appends structured logs.

Notes:

- Problems: `ChronoUtc::rfc3339()` API mismatch; corrected to `ChronoUtc::rfc_3339()` using Context7 docs.
- Derivations: Persisted `WorkerGuard` in a `OnceLock` to ensure non-blocking appender flushes at process end; wired logging init in `apps/orchestra/main.rs` before CLI dispatch.

## [x] Phase 6: JSON-RPC transport skeleton (daemon)

- What to do: Start a minimal daemon using `hyper` + `hyperlocal` + `jsonrpsee`. Expose `daemon.status` returning version/pid/socket path.
- Testing strategy: Integration test that starts daemon bound to a temp UDS path, sends JSON-RPC call, asserts response and logs written.
  NOTE: Make sure to save the socket in a tmp folder during testing to not impact the global system. Add a config if necessary.
- Feedback loop: `orchestra daemon start` then `orchestra daemon status` works. (Cleanup afterwards)

Notes:

- Problems: hyper v1 requires wrapping `UnixStream` with `hyper_util::rt::TokioIo` and using `http_body_util::Full<bytes::Bytes>` for bodies. Adjusted server and tests accordingly. Also used `hyperlocal::Client::unix()` from hyper-util legacy to create a UDS client.
- Derivations: Skipped `jsonrpsee` for now in favor of a hand-rolled minimal JSON-RPC 2.0 handler to keep footprint small; will revisit in later phases if needed.

## [ ] Phase 6.1: Replace handrolled JSON RPC handling with jsonrpsee

- In Phase 6, we handrolled our own JSON RPC implementation
- Replace it with `jsonrpsee` (like originally planned) to improve correctness and maintainability
- Use Context7 to understand how to use `jsonrpsee` effectively
- Make sure to keep the Context7 section ./AGENTS.md updated with all dependency changes

## [ ] Phase 7: CLI RPC client + basic UX

- What to do: Add `jsonrpsee` UDS client, `clap` args, and simple styling for `daemon status`. Friendly error mapping.
- Testing strategy: Snapshot tests for `orchestra daemon status` output; unit tests for error mapping and arg parsing.
- Feedback loop: Nice colored output for `daemon status`.

## [ ] Phase 8: Git adapter (worktrees/branches) and init scaffolding

- What to do: Implement `git2` helpers for base branch checks and worktree creation. Add `orchestra init` to scaffold `.orchestra` and default configs.
- Testing strategy: Temp repo integration tests that run `init` and inspect created files; unit tests for branch naming and worktree path logic.
- Feedback loop: `orchestra init` prepares the project reliably.

## [ ] Phase 9: Task lifecycle API (new, status, start – stub)

- What to do: Implement `task.new` (create task file), `task.status` (list), and a stub `task.start` that validates/preps git state and sets `running` (no PTY yet). Record `base_sha` in events only.
- Testing strategy: Integration tests that drive `new/status/start` via RPC; assert YAML content, events, and status transitions.
- Feedback loop: End-to-end draft→running path without PTY.

## [ ] Phase 10: PTY backend and attach/detach

- What to do: Add `portable-pty` adapter and `pty.attach/input/resize` RPCs with single active attachment. Initially spawn a simple echo program for determinism.
- Testing strategy: Integration test that attaches, sends input, receives expected output; resize event recorded.
- Feedback loop: `orchestra attach <task>` shows live PTY output.

## [ ] Phase 11: Idle detection with dwell and signaling

- What to do: Implement idle transitions (10s threshold, 2s dwell). Any PTY output resumes `running`. Add `orchestra idle <id|slug>` to manually toggle.
- Testing strategy: Time-controlled tests using a mock clock or short thresholds in test config; assert transitions and debouncing.
- Feedback loop: Logs show stable `idle ↔ running` without flapping.

## [ ] Phase 12: Fake agent adapter for deterministic tests

- What to do: Provide a `fake` adapter configured via `.orchestra/agents/fake.toml` that prints deterministic content and supports resume.
- Testing strategy: E2E tests in temp repo drive `new/start/attach/idle` with fake agent; snapshots for PTY output.
- Feedback loop: Fully deterministic agent-driven flows.

## [ ] Phase 13: Complete/review/fail + merge (squash) and cleanup

- What to do: Implement `task.complete`, `task.fail`, `task.reviewed`, and `task.merge` with default squash; on success set `merged`, remove worktree/branch.
- Testing strategy: Integration tests exercising merge flow into temp repo branches; assert branch state, status updated, and artifacts removed.
- Feedback loop: Full lifecycle to `merged` works reproducibly.

## [ ] Phase 14: GC, path, and shell-hook QoL

- What to do: Add `orchestra gc`, `orchestra path <task>`, and `orchestra shell-hook` outputs; confirm destructive prompts unless `-y`.
- Testing strategy: CLI tests verifying printed paths and shell hook content; GC dry-run snapshot, then `-y` execution assertions.
- Feedback loop: Cleanups and navigations are smooth.

## [ ] Phase 15: MCP bridge and `mcp` subcommand

- What to do: Implement MCP server in `mcp` crate using Rust SDK, forwarding to daemon RPCs. Add `orchestra mcp` subcommand.
- Testing strategy: Start MCP server in test, call minimal handlers against daemon, assert correct bridging.
- Feedback loop: External MCP clients can list tasks and attach PTY via MCP.

## [ ] Phase 16: Polish, defaults, and CLI snapshots

- What to do: Ensure ADR defaults (timeouts, logging, squash) are honored; refine CLI tables/colors; finalize docs. Ensure RPC payloads use `snake_case` and include `version: u8`.
- Testing strategy: Snapshot tests for CLI; config conformance tests; quick end-to-end suite in CI.
- Feedback loop: `just test` green with stable UX and defaults.
