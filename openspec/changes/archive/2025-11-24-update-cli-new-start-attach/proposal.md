# Proposal: Update CLI `agency new` to start and attach without opening editor

## Summary
Change the default behavior of the CLI command `agency new <slug>` so that it no longer opens the editor. Instead, it MUST immediately create the task, start its session, and attach to it (unless the user explicitly requests draft or no-attach behavior).

## Motivation
- Reduce friction for the common workflow where users want to jump directly into the agent session and iterate on instructions there.
- Align CLI behavior more closely with the TUI "New + Start" flow, while still honoring explicit draft and no-attach flags.
- Make `agency new` safer and more predictable in scripted or non-interactive contexts by avoiding editor launches by default.

## Scope
- CLI behavior for `agency new` only.
- Interactions with existing flags: `--draft`, `--no-attach`, `--agent`, `--description`, and the optional positional description.
- No changes to TUI behavior beyond how it may rely on shared command helpers.
- No changes to daemon protocol or attach implementation beyond what is required to call existing APIs.

## High-Level Changes
- Redefine the default behavior of `agency new <slug>` to:
  - Create the task immediately with an empty (or explicitly provided) description.
  - Start the task session.
  - Attach to the task session, unless `--no-attach` or `--draft` is set.
- Preserve `agency new --draft <slug>` as the way to create a task without starting or attaching, and explicitly keep the existing behavior that opens the editor in interactive TTY mode when no description is provided.
- Ensure `--description` and the positional description both bypass any editor usage, while still respecting draft and attach flags.
- Document the new behavior and expected flows in the CLI behavior spec and README in a follow-up implementation change.

## Risks & Trade-offs
- Users currently relying on `agency new <slug>` to open the editor may be surprised by the new attach-first behavior; mitigated by keeping `--draft` and `agency edit` as explicit flows.
- Changing defaults can affect scripts or tooling; however, scripts should already be using `--draft` or `--description` to avoid editor prompts. The new behavior is safer in non-interactive contexts because it avoids unexpected editor launches.
- Implementation must carefully separate CLI-level behavior from shared helpers (`commands::new::run`) so TUI behavior is not regressed.

## Validation
- Add CLI integration tests that cover:
  - `agency new <slug>` in an interactive-like environment starts and attaches without opening an editor.
  - `agency new --draft <slug>` still uses the editor (when configured) and does not start/attach.
  - `agency new --description ...` creates the task with the provided description and respects `--draft`/`--no-attach` semantics.
- Run `openspec validate update-cli-new-start-attach --strict` and the full `just test` / `just check-strict` pipeline after implementation.
