# Tasks: Update TUI start/new to no-attach

## Ordered Checklist

- [x] 1.1 Update `s` Start action to call `start::run_with_attach(ctx, ident, false)`
- [x] 1.2 Update `New + Start` (`N`) to call `start::run_with_attach(ctx, id, false)`
- [x] 1.3 Revert helper text changes (no help updates)
- [x] 1.4 Run `openspec validate update-tui-start-new-no-attach --strict`
- [x] 1.5 Run `just check-strict` and `just test` to ensure no regressions

## Notes
- Ensure log pane shows feedback for background starts.
- Keep changes minimal and isolated to TUI module.
