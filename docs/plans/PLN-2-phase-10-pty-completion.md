# PLAN-2: Phase 10 – PTY backend completion (attach/detach) and manual testability

Date: 2025-08-31

## Decisions (confirmed)

- Default detach sequence: Ctrl-q (C-q). Configurable via config (`pty.detach_keys`) and env `AGENCY_DETACH_KEYS`. No CLI flag required.
- `pty.attach` may be invoked when the task is not `running`, but the daemon must respond with a clear error describing the required state.
- PTY shell: spawn plain `sh` (no `-l`) for deterministic output and stable tests.

## Scope

Deliver a complete, test-backed PTY attach/detach implementation with a clear lifecycle and manual CLI flows:

- Provide CLI commands to create and start tasks: `agency new`, `agency start`, `agency status`.
- Enforce lifecycle boundaries: PTY session created on `task.start`; `pty.attach` requires `running`.
- Stable PTY behavior and bounded buffers; correct resize and single-attachment semantics.
- Fill the test gaps at core (daemon) and CLI levels.
- Align docs (ADR/PRD) around defaults and add `pty.read` call (polling) to interfaces.

## Changes by module

### crates/core (daemon, adapters, config)

- `src/config/mod.rs`
  - Add `pty.detach_keys: Option<String>` to `Config` and `PartialConfig`.
  - Default: `None` (implicitly default to `ctrl-q` at use sites if not set).
  - Merge logic: standard override semantics (project > global > defaults).
  - Include the field when writing default project config.

- `src/adapters/fs.rs`
  - No structural change; confirm worktree path helper is available or add: `worktree_path(project_root, id, slug)` to resolve cwd for PTY child. If missing, derive from `worktrees_dir()` as `worktrees/{id}-{slug}`.

- `src/adapters/pty.rs`
  - Spawn PTY only from `task.start` (remove implicit spawn from `attach`; validate session existence instead).
  - Change spawned program to `sh` (no `-l`).
  - Set child cwd to the task worktree path.
  - Retain child process handle in `PtySession` for lifecycle control.
  - Implement bounded output buffer (e.g., cap ~1 MiB; on append, drop oldest to maintain cap).
  - Keep single-attachment invariant; error on second `attach`.
  - Add structured errors (`anyhow` context strings) for better RPC mapping.

- `src/daemon/mod.rs`
  - `task.start`: ensure/compute the task worktree path; spawn PTY session (calls `pty::ensure_spawn(root, id, worktree_path)`).
  - `pty.attach`: enforce that task’s YAML status is `running`; if not, return JSON-RPC error `-32010` with message like "cannot attach: task is not running (status: X)". Do not spawn here.
  - `pty.*` methods: keep existing wiring for `read`, `input`, `resize`, `detach`; improve error codes/messages consistently across the set (`-32010`..`-32014`).
  - Document `pty.read` as part of the MVP polling model in comments.

### crates/cli (args, lib, rpc)

- `src/args.rs`
  - Add subcommands: `New`, `Start`, `Status`.
  - Remove `--detach-keys` from `Attach` (config/env only). Help text: print detach hint and mention config/env override.

- `src/lib.rs`
  - Implement handlers for `new`, `start`, `status`:
    - `new`: construct `TaskNewParams` (accept `--base-branch`, optional `--label` repeats, `--agent` enum with default `fake` for tests), call RPC, print created id/slug.
    - `start`: accept `<id|slug>`, call RPC, print new status.
    - `status`: list tasks in a simple, deterministic table/text (suitable for tests).
  - `attach` flow:
    - Determine detach sequence from `Config.pty.detach_keys` or env `AGENCY_DETACH_KEYS`; default to `ctrl-q`.
    - Keep raw-mode input, filtering detach bytes locally; poll `pty.read`; on detach, call `pty.detach` and exit with message "detached".

- `src/rpc/client.rs`
  - Keep the manual HTTP/UDS JSON-RPC client; ensure robust Unix Domain Socket support (e.g., via `hyperlocal`).
  - Provide typed wrappers for `task.new`, `task.start`, `task.status`, and existing `pty.*`.
  - Map transport/JSON-RPC errors to CLI errors; preserve server-provided messages for UX clarity.

### docs

- `docs/adr/ADR-1-mvp.md`
  - Resolve default detach sequence to Ctrl‑q (C‑q) and remove CLI flag reference.
  - Add `pty.read` to the `pty.*` method list, note it’s a polling MVP.

- `docs/prd/PRD-1-agency-v1.md`
  - Align detach default (Ctrl‑q) and emphasize config/env override only.
  - Mention `pty.read` polling.

- `docs/plans/PLN-1-agency-mvp.md`
  - Keep Phase 10 unchecked; add a short note referencing this plan (PLAN-2) for completion steps.

## Testing strategy

- Core integration tests (new file: `crates/core/tests/pty.rs`)
  - Setup: temp git repo, start daemon (existing harness pattern).
  - Flow: `task.new` → `task.start` → `pty.attach` (rows/cols) → `pty.input` ("echo hi\n") → `pty.read` asserts output contains "hi".
  - `pty.resize`: call and assert success (no crash).
  - Errors:
    - Double `pty.attach` returns an error.
    - `pty.attach` when status != `running` returns error with clear message.
    - `pty.read`/`pty.input` after `pty.detach` returns error.

- CLI E2E tests (`crates/cli/tests/attach_e2e.rs`)
  - Spawn daemon via CLI (`daemon start`).
  - Run `init`, `new`, `start` using CLI commands; then `attach`.
  - Drive input to stdin programmatically: send `echo hi\n`, assert output contains `hi`, then send detach bytes for Ctrl‑q; assert process exits and prints "detached".
  - Verify no CLI flag for detach; add a test that sets `AGENCY_DETACH_KEYS=ctrl-p,ctrl-q` and validates detach behavior.

- Config tests (`crates/core/src/config/mod.rs`)
  - Validate `pty.detach_keys` loads/merges correctly; env override honored by CLI.

## Acceptance criteria

- `just check` passes; `just test` passes.
- `agency` provides `new`, `start`, `status`, `attach` commands; manual flow is usable end‑to‑end in a temp git repo.
- Attaching to non‑running tasks produces a clear error.
- Default detach is Ctrl‑q; configurable via config/env; CLI has no `--detach-keys`.
- PTY output is deterministic enough for stable tests (no login shell noise).
- PTY output buffer is bounded to ~1 MiB.
- CLI uses a custom HTTP/UDS JSON-RPC client with UDS support.
- ADR/PRD references match implementation (defaults + `pty.read`).

## Risks and mitigations

- PTY timing flakiness:
  - Mitigation: Poll with small delays and allow short waits in tests; avoid strict exact match of shell prompts; assert substrings like `hi`.
- Buffer bounding complexity:
  - Mitigation: Simple ring-buffer behavior by truncating from the front when exceeding cap while holding the lock; keep code minimal.

## Tasks (execution checklist)

1. [x] Config: add `pty.detach_keys` to `Config` (+ merge, defaults, tests).
2. [x] Core: worktree path helper; PTY spawn in `task.start` only; enforce `running` on `pty.attach`.
3. [x] CLI: keep/strengthen manual HTTP/UDS JSON-RPC client with hyperlocal UDS support; add typed wrappers and port all calls.
4. [x] CLI: add `new`, `start`, `status` subcommands; remove `--detach-keys` from `attach`.
5. [x] Core: PTY behavior tweaks (plain `sh`, cwd to worktree, bounded buffer, keep child handle).
6. [x] Tests: core PTY integration tests; CLI E2E attach tests; config merge tests for detach keys.
7. [x] Docs: update ADR/PRD (defaults, `pty.read`) and add a note in PLN‑1 Phase 10.
8. [x] Run `just check` and `just test`; fix any lints/clippy issues.

## Out of scope (follow-ups)

- Idle detection (Phase 11) and fake agent adapter (Phase 12).
- Multiple simultaneous attachments.
- Agent-specific PTY behaviors or shells beyond `sh`.
