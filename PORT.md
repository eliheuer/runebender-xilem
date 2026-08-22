# Porting Runebender to xix

This file logs what the port forces into the framework, in order. It is
the feedback loop from DESIGN.md section 11 ("Runebender under 5,000
lines at PARITY.md"). Reference: runebender-gpui (13,809 lines, one
file) and its `PARITY.md`.

## Log

- 2026-08-22: repository created. First slice: open a UFO or the first
  master of a designspace through `runebender-core`, list glyphs, edit
  one glyph's outline in a canvas island (pan, zoom, select, drag).

  Done the same day: `src/main.rs`, 630 lines, renders headless
  (`XIX_SCREENSHOT`). What it forced, in order of cost:

  1. **Two implementations per island.** The canvas needs a Masonry
     `Widget` impl (measure, layout, paint, pointer events,
     accessibility, children) and a Xilem `View` impl (build, rebuild,
     teardown, message) plus an event type and a constructor: about
     140 lines before any drawing. DESIGN.md 5 ("one implementation
     per widget") is confirmed as the first thing to fix.
  2. **Style wrappers change the type.** `.dims(..)` is not available
     on an `impl WidgetView` return value, so the sidebar is wrapped in
     `sized_box(..).fixed_width(..)`. Same trap as calc's `.color()`.
  3. **No virtualized grid.** The glyph list is 400 `text_button`s in
     a `portal`; it needs the glyph grid the gpui port has (ink-box
     cells, mark colors, categories). This is the first real part to
     build.
  4. **Theme bridge is hand-written.** Surfaces, text, and roles from
     the shared OKLCH file are mapped by name into a `Palette`; stock
     widgets (buttons, inputs) still use Masonry's default property
     set, so the sidebar does not match the canvas.
  5. **Core gap.** `runebender_core::path::Path` has `points()` but no
     mutable accessor; moving points means matching on the variant
     and calling `points.make_mut()`. Core should own "translate these
     point ids by this delta" (it has `point_ops::translate_points`
     for the norad form; the `Path` form needs the same).
  6. **Keyboard is untouched.** No nudge, no delete, no undo yet.
     Window-level shortcuts are the known Xilem gap (PLAN.md 4.1).

  What worked without friction: `norad::Font::load`, designspace first
  master, `Path::from_contour` and `append_to_bezpath`, `ViewPort`
  (fit, pan, zoom-about), `Painter::fill/stroke(..).draw()` with kurbo
  shapes, `EventCtx::local_position`, `capture_pointer`,
  `submit_action`, and the headless screenshot path.

- 2026-08-22, slice 2: **glyph grid + mode switch.** Refactored into
  modules (`theme`, `model`, `grid`, `editor`, `main`). Added a cached
  `FontModel` (per-glyph outline, ink box, mark, category) and a
  `GridWidget` island that paints every visible cell into one scene,
  scrolls, selects, and reports open/select events. Click selects,
  click again opens the editor; `‹ Overview` returns. Verified on a
  657-glyph font (Newsreader). Forced:
  - **A third canvas island already repeats itself.** Grid and editor
    both hand-roll: measure/layout/paint, scroll math, pointer routing,
    the View wrapper, an event enum, `submit_action`. The canvas kit in
    DESIGN.md 6 (viewport, gesture delegate, scene, hit testing) would
    remove most of both files. This is now the highest-value framework
    piece.
  - **No virtual grid widget.** The grid is hand-virtualized (only
    visible rows painted). Fine as an island, but a general
    `virtual_grid` part would serve the media bin, asset browser, etc.
  - **Mode switch is `one_of::Either`.** Works, but the two arms must
    have the same `State`; fine here since both are `App`.
  - **Cell labels still missing** (name, codepoint): needs text in the
    scene, i.e. `Painter::glyphs` with a shaped line. Next slice.

