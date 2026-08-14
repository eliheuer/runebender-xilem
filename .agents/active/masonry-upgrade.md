---
slug: masonry-upgrade
agent: Claude Code (eli@laptop)
branch: agent/scope-masonry-upgrade
worktree: ~/Temp/worktrees/runebender-xilem-scope-masonry-upgrade
started: 2026-08-07
last_touched: 2026-08-07
touches:
  - Cargo.toml
  - src/components/
  - src/views/
  - src/tools/
  - src/editing/
  - src/sort/
  - src/theme.rs
  - src/lib.rs
---

## Goal

The real Masonry upgrade, continuing from the scoping branch. Done
means `cargo check` passes against xilem git main (kurbo 0.13
stack) and the app runs. Plan and error census are in
`.agents/MASONRY-UPGRADE.md`.

## Status

- [x] Claim filed
- [ ] Bucket 1: import reorg (~112 errors)
- [ ] Bucket 2: Length unit type (~99 errors)
- [ ] Bucket 4: context/view renames, parley 0.8, flex Prop API
- [ ] Bucket 3: `measure` on 17 custom widgets
- [ ] linesweeper kurbo boundary shim + img2bez drift
- [ ] cargo check clean, app launches

## Notes

Reference checkout of xilem main (rev 7819435) lives in the session
scratchpad. Follow-up after this lands: move path/editing/model into
runebender-core and start porting runebender-web features back.
