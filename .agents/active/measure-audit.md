---
slug: measure-audit
agent: Claude Code (eli@laptop)
branch: main (working directly; Eli is driving interactively)
worktree: none (main checkout)
started: 2026-08-17
last_touched: 2026-08-17
touches:
  - src/components/mod.rs (measure_fill)
  - src/components/*/ (measure impls on custom widgets)
  - src/views/editor.rs (multi_glyph_view 10000 preview)
  - src/components/glyph_preview_widget.rs
---

## Goal

Fix the live-window misrender (see UI-PARITY.md blocker section):
audit every custom widget's `measure` so no widget reports huge or
offered space as its content size. Done means the grid tab has no
right-edge overflow and the editor tab renders toolbars, panels,
and the glyph live (verified by screenshots), with headless render
tests still passing.

## Status

- [x] Claim filed
- [ ] Study masonry measure contract (MinContent/MaxContent/FitContent)
- [ ] Fix multi_glyph_view 10000 preview + measure_fill
- [ ] Audit remaining custom widget measure impls
- [ ] Live screenshots: grid tab + editor tab correct
- [ ] cargo test render_ still passes

## Notes

Root cause analysis in `.agents/UI-PARITY.md` (2026-08-17 evening
update). Screen Recording is granted; `RB_OPEN_GLYPH=<name>` env
hook opens the editor tab directly for screenshot QA.
