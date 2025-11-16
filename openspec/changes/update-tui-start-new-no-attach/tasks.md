# Tasks: Update TUI start/new to no-attach

## Ordered Checklist

- [ ] 1.1 Update `s` Start action to call `start::run_with_attach(ctx, ident, false)`
- [ ] 1.2 Update `New + Start` (`N`) to call `start::run_with_attach(ctx, id, false)`
- [ ] 1.3 Refresh help text to indicate background start
- [ ] 1.4 Run `openspec validate update-tui-start-new-no-attach --strict`
- [ ] 1.5 Run `just check` and `just test` to ensure no regressions
- [ ] 1.6 Document behavior in `README.md` TUI section

## Notes
- Ensure log pane shows feedback for background starts.
- Keep changes minimal and isolated to TUI module.

