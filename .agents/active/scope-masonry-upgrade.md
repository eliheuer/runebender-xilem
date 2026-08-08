---
slug: scope-masonry-upgrade
agent: Claude Code (eli@laptop)
branch: agent/scope-masonry-upgrade
worktree: ~/Temp/worktrees/runebender-xilem-scope-masonry-upgrade
started: 2026-08-07
last_touched: 2026-08-07
touches:
  - Cargo.toml
  - Cargo.lock
---

## Goal

Scoping only, not the upgrade itself. Point this repo's deps at
xilem git main (kurbo 0.13.1, vello 0.8, wgpu 28, peniko 0.6,
parley 0.8 — the same stack runebender-web core uses), run
`cargo check`, and produce an error census: how many errors, in
which modules, grouped by cause (kurbo type split, masonry crate
split, xilem view API churn, winit/ui-events changes). Output is a
written scope report in `.agents/MASONRY-UPGRADE.md` to plan the
real upgrade.

## Status

- [x] Claim filed
- [ ] Worktree created, deps switched to git main
- [ ] cargo check error census
- [ ] Scope report written

## Notes

Context: runebender-web is the primary editor; this repo is dormant
and expected-broken, so nothing downstream depends on it building.
The upgrade unlocks moving kurbo-typed modules into runebender-core
so both editors share them. Latest crates.io xilem/masonry release
is still 0.4 (2025-10-29); git main is ~10 months ahead and includes
the masonry_core/masonry_winit split.
