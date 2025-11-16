## 1. Protocol and daemon (event-driven)
- [ ] 1.1 Add `D2CControl::TuiFollowersChanged { project, tui_id, followers }`
- [ ] 1.2 Maintain follower count on `TuiFollow` connect/disconnect (close decrements)
- [ ] 1.3 Optionally add `followers: u32` to `TuiListItem` for snapshots
- [ ] 1.4 Unit tests: follower count increments/decrements and event broadcast

## 2. TUI behavior under follow
- [ ] 2.1 Subscribe to follower-change events; track local follower count for own `tui_id`
- [ ] 2.2 Bypass auto-attach for TUI create/start while followers > 0
- [ ] 2.3 Keep list focus and highlight started task

## 3. Follow integration
- [ ] 3.1 Ensure follower attaches to focused task upon start
- [ ] 3.2 Do not interfere with follower child process lifecycle

## 4. Preserve CLI defaults
- [ ] 4.1 Verify non-follow scenarios retain current auto-attach behavior

## 5. Validation
- [ ] 5.1 Unit tests only: TUI attach-defer decision logic and event handler updates
- [ ] 5.2 Run `just check`, `just test`, `just check-strict`
