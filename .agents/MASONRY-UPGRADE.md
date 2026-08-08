# Masonry upgrade scope report

Date: 2026-08-07. Produced by the `scope-masonry-upgrade` task.

## Goal

Upgrade this repo from xilem/masonry 0.4 (crates.io, kurbo 0.12) to
xilem git main. This puts the repo on the same graphics stack as
`runebender-web/core`: kurbo 0.13.1, vello 0.8, wgpu 28, parley 0.8,
peniko 0.6. After the upgrade, the kurbo-typed editing and model
modules can move into `runebender-core` so both editors share them.

## Method

Branch `agent/scope-masonry-upgrade` points `Cargo.toml` at
xilem git rev `7819435e592f3b3dda75432740264ef5fc1ddaa2` (main,
2026-08-07) and bumps kurbo to 0.13, parley to 0.8, peniko to 0.6.
Then `cargo check`. The branch is a valid starting point for the
real upgrade. Note: the worktree needed absolute paths for the
`runebender-core` and `img2bez` path deps.

## Result: 306 errors, all in the UI layer

Dependency resolution succeeds. The error count by module:

| Module | Errors |
|---|---|
| `src/components/` | 169 |
| `src/views/` | 97 |
| `src/tools/` | 26 |
| `src/editing/` | 10 |
| `src/sort/` | 2 |
| `src/theme.rs`, `src/lib.rs` | 2 |
| `src/path/`, `src/model/`, `src/data/` | **0** |

The headline: **the shareable logic already compiles on kurbo
0.13.** `src/path/`, `src/model/`, and `src/data/` have zero
errors. Of the 10 errors in `src/editing/`, six are unrelated
`img2bez` API drift and two are the known linesweeper kurbo-0.12
boundary in `session/path_editing.rs:1007,1043` (fix with a small
0.12 to 0.13 point conversion shim, same as runebender-web does).

So the old "~289 kurbo split errors" problem is gone once masonry
itself is on kurbo 0.13. Moving `path/`, `editing/`, and `model/`
into `runebender-core` after the upgrade should be close to a
straight move.

## Error categories (the actual upgrade work)

1. **Import reorg, ~112 errors, mechanical.** `masonry::vello` no
   longer exists (32 sites). `xilem::core::*` is now `xilem_core`
   (16). `masonry::core::*` is now `masonry_core` (16).
   `masonry::properties::types::*` moved (7). `masonry::util::
   fill_color` and `stroke` moved or renamed (6).
2. **Length unit type, ~99 errors, mechanical.** Sizes are now the
   `Length` type. Errors are `expected Length, found f64` plus
   unresolved `.px()` calls (the px extension trait moved; import
   the new one and convert raw f64 call sites).
3. **Widget trait, 17 errors, the real work.** Every custom widget
   must implement a new required `measure` method. This repo has 17
   custom widgets (editor canvas, glyph cells, toolbars, panels).
   Formulaic but needs per-widget thought about intrinsic sizing.
4. **Context and view API churn, ~60 errors.** `PaintCtx::size`
   renamed (14 sites), `ViewCtx::record_action` gone (13), flex
   views now configured through a `Prop` builder
   (`cross_axis_alignment` and friends, ~16),
   `CrossAxisAlignment::Fill` removed (4), parley 0.8 renamed
   `FontFamily::Generic` and `StyleProperty::FontStack` (18).
5. **Misc.** `img2bez` API drift in `src/editing/tracing.rs` (6),
   app-runner closure signature in `src/lib.rs:136` (1).

Estimate: days of focused work, not weeks. The bulk is find-and-
replace; the `measure` implementations and the flex `Prop` API are
the parts that need real decisions.

## Upstream xilem contribution targets

- No wasm/browser support in `masonry_winit` today, and no wasm
  example in the repo, even though `masonry_core` already carries
  wasm32-only deps and winit itself supports canvas-backed windows.
  A web driver (or a `masonry_winit` web target) is a well-scoped
  contribution. The vello + wgpu 28 render stack is already proven
  in the browser by `runebender-web/core`.
- Migration pain found during categories 2 to 4 (missing docs,
  awkward APIs) is direct issue and PR material. Note xilem main is
  10 months ahead of the last crates.io release (0.4, 2025-10-29),
  so file issues against main, not the release.

## Suggested order for the real upgrade

1. Rebase or branch from `agent/scope-masonry-upgrade`.
2. Fix imports (category 1) and Length (category 2) first; both are
   mechanical and shrink the error list fast.
3. Fix parley 0.8 and context renames (category 4).
4. Implement `measure` for the 17 widgets (category 3), starting
   with simple panels, ending with `editor_canvas`.
5. Fix the linesweeper boundary shim and `img2bez` drift.
6. Then: move `path/`, `editing/` (kurbo parts), and `model/` into
   `runebender-core`; adopt the web core's newer versions where they
   diverge. License note: web is GPL-3.0, core is Apache-2.0, so
   relicensing web-derived code needs a deliberate decision by Eli.
