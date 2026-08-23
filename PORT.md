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

- 2026-08-22, slice 3: **keyboard editing and undo on the island.**
  Arrow keys nudge the selection (shift = x10), Delete/Backspace remove
  selected points, Cmd+A selects all, Cmd+Z / Cmd+Shift+Z undo/redo
  (core `UndoState<Vec<Path>>`), Escape returns to the overview. Forced,
  and this is the important finding:
  - **Widget-focused keys work; window shortcuts do not.** Masonry
    routes key events to the focused widget, so editing keys land once
    the canvas has focus (it grabs focus on pointer down). That covers
    tool keys and nudges. It does NOT cover global shortcuts like Cmd+S
    or menu accelerators that must fire regardless of focus, which is
    the documented Xilem gap (PLAN.md 4.1, the zstack workaround). So
    the shortcut story splits cleanly: island-local keys are fine today;
    a window-level action/keymap/menu layer is still the framework's to
    build (DESIGN.md 9).
  - **Undo lives in the island**, as in runebender-xilem, because the
    view layer would rebuild over it. A framework undo tied to a typed
    command stack would let this move out of the widget.
  - Noticed an orphaned `session.rs` (a norad-glyph-based session using
    `glyph_ops`/`point_ops`) committed but not wired in; removed here.
    The editor's `Path`-based session stays for now; moving to the
    norad-glyph session (which unlocks real ops: boolean, decompose,
    transforms, save) is a planned refactor.

- 2026-08-22, slice 4: **adopt the norad-glyph session.** The editor
  island now owns a `session::Session` that works on `norad::Glyph`
  directly, so `runebender_core::glyph_ops` and `point_ops` apply with
  no conversion. Undo is snapshot-based (`glyph_ops::snapshot/restore`),
  drags measure from the gesture origin via `translate_points` with
  drag-originals (no snap drift), and edits flow back to the grid cache
  (`FontModel::replace_glyph`) so the overview preview updates. Start
  nodes and resolved components now draw. This is the base every real
  op (boolean, decompose, transform, save) builds on. Forced:
  - **The edit/refresh round-trip is manual.** On `Edited`, the app
    clones the glyph out of the session, writes it back into the font
    and the cell cache. A framework document/session abstraction would
    own this; here it is app code, but small.
  - `Arc::get_mut` on the font works only because nothing else holds the
    font Arc; the norad-glyph session clones the glyph, not the font.
    Multiple live sessions (tabs) will need the font behind a shared
    cell or an explicit commit step.

- 2026-08-22, slice 5: **Pen tool.** A `Tool { Select, Pen }` enum on
  the app, Select/Pen buttons in the editor toolbar, and pen handling
  in the island: click places corner points (`glyph_ops::start_contour`
  / `append_segment`), clicking near the first point closes
  (`close_contour`), a faint accent segment previews from the last
  point to the cursor, Escape cancels the open path. Verified headless
  by scripting a triangle. Line-only for now; click-drag for bezier
  handles is a later slice. Forced:
  - **Tool state is a rebuild input.** The tool lives on the app and is
    threaded into the island through the view; switching tools rebuilds
    and cancels any open pen path. Clean, but it is the pattern the
    canvas kit should formalize (a tool is a state machine with a
    cursor, an overlay, and a keymap scope, DESIGN.md 6).

- 2026-08-22, slice 6: **save.** `FontModel::save` writes the norad
  font back to its UFO; the app flushes the open glyph first
  (`refresh_open_glyph`) so in-progress edits are included. A Save
  button in the toolbar works in any mode; Cmd+S works in the editor.
  A modified dot (`Save •`) and a "Saved <path>" status line give
  feedback. Verified on disk: a scripted pen triangle round-trips
  (space.glif 0 → 1 contour, 3 points, reloads). Forced, and this is
  the recurring finding:
  - **Save is the clearest case of the window-shortcut gap.** Cmd+S
    only fires when a focusable island has focus, so it is wired in the
    editor and would need wiring in the grid too. The toolbar button is
    the reliable cross-mode path. A window-level action + keymap + menu
    layer (DESIGN.md 9) is the missing framework piece; every app works
    around it the same way.

- 2026-08-22, slice 7: **sidebar with search and category filter.**
  A left panel in overview mode: a search box (name or unicode) and
  category rows (All, Letter, Number, Punctuation, Symbol, Mark, Other)
  with live counts, driven by core's `GlyphCategory`. The grid selects
  by font index now, so filtering the visible set does not break
  selection or open. Verified: Number narrows 657 glyphs to the 18
  numerals. Forced:
  - **Filtering is app state, recomputed each view.** `filtered_cells`
    runs every rebuild; fine at 657 glyphs, but the pattern (derive a
    filtered projection, feed the island) is what a `virtual_grid`
    part plus a selection model should own.
  - Language groups, GF coverage, sort modes, and search scopes/regex
    from gpui's sidebar are still to do; this is the base.

Parity estimate after slice 7: the overview, select + pen tools,
viewport, point editing, undo, save, and a filtering sidebar are in.
Roughly a quarter of runebender-gpui's surface. Remaining large blocks:
bezier pen (curve handles), components/anchors, boolean/transform ops,
measure/curvature tools, kerning + text tool, designspace masters +
interpolation, native menus + window shortcuts, and the browser build.

- 2026-08-22, slice 8: **bezier pen (curve handles), ported from
  runebender-xilem.** The pen now distinguishes click from click-drag:
  a click places a corner on-curve point; a click-drag places a smooth
  on-curve point with symmetric off-curve handles, the outgoing handle
  following the cursor and the incoming handle mirrored. Logic lifted
  from `runebender-xilem/src/tools/pen.rs` (which is the same
  Xilem/Masonry family) and rewritten onto norad points via a pen
  buffer in the session (`pen_corner`, `pen_smooth_begin/drag`,
  `pen_close`). Verified headless: a smooth point produces a real cubic
  arch (blue on-curve, orange/purple handles, 5 points, closes to a
  filled contour). Note:
  - runebender-xilem is the best source for the remaining tools
    (HyperPen, Knife, Measure, Shapes, Select marquee) and its
    `mouse.rs` gesture recognizer, because it targets the same
    framework. Porting is "adapt its `Path`/`MouseDelegate` logic to
    norad + the island's pointer handlers", not a rewrite.

- 2026-08-22, slice 9: **marquee selection.** In the select tool, a
  primary drag on empty space draws a rubber-band rectangle
  (`selection` role, translucent) and selects the points inside on
  release; shift adds to the selection. Middle/secondary drag still
  pans. Build-verified; the drag interaction is not screenshot-checked
  (headless can't drag), but the rect-contains logic is trivial and the
  selection rendering is already proven.

- 2026-08-22, slice 10: **anchors display.** Glyph anchors
  (`glyph.anchors`) draw as small accent diamonds. Verified on "A"
  (top/bottom/ogonek anchors on the baseline). Editing anchors (add,
  move, name, delete) is a later slice; this is display only.

- 2026-08-22, slice 11: **Shapes tool (rectangle, ellipse), ported
  from runebender-xilem.** Rect and Ellipse tools with a drag gesture
  and live preview; on release the session adds a closed contour
  (`add_rect` = 4 corner Line points; `add_ellipse` = 4 smooth Curve
  on-curves with 8 off-curve controls at the 0.5523 kappa). Logic from
  `runebender-xilem/src/tools/shapes.rs`. Verified: a rect and a proper
  cubic ellipse (16 points). Shift-lock (square/circle) is a later
  refinement.

- 2026-08-22, slice 12: **transform and boolean ops (from the gpui
  port's command set).** Session methods over core: `flip_horizontal`
  / `flip_vertical` / `rotate_90` (via `transform_selection` with a raw
  affine, which core centers on the selection bbox), `reverse`
  (`reverse_contours`), and `remove_overlap` (core's linesweeper
  union). Bound to unmodified keys in the editor (h/v/r/]/o) for now;
  these belong on a menu once the window-level action layer exists.
  Verified: an overlapping rect + ellipse union into one 19-point
  contour. Boolean subtract/intersect/exclude are the same call with a
  different `linesweeper::BinaryOp`; a later slice adds them plus
  decompose.

- 2026-08-22, slice 13: **boolean subtract/intersect/exclude and
  decompose.** `session.boolean(BoolOp)` maps a local enum to
  `linesweeper::BinaryOp` and runs `glyph_ops::boolean_contours`;
  `session.decompose()` turns components into editable contours.
  Component contours are precomputed at session creation
  (`resolve_components`) because the whole `norad::Font` is not
  Send/Sync (its datastore holds a `RefCell`), so the session cannot
  hold the font; it holds the resolved contours instead. Verified: Á
  (A + acute, 2 components) decomposes into 3 contours, 25 editable
  points. Note the Send/Sync constraint is a real one for this
  architecture: island widgets must be Send, so anything they own
  must be too.

Parity after slice 13 (~a third of runebender-gpui): overview grid +
sidebar, select/pen/rect/ellipse tools, marquee, point editing, undo,
save, flips/rotate/reverse, boolean ops, remove-overlap, decompose,
anchor + component display. Remaining: HyperPen/Knife/Measure tools,
anchor + component editing, kerning + text/shaping tool, designspace
masters + interpolation, curvature/measure overlays, native menus +
window shortcuts, and the browser build (feasible via the xix web
driver). runebender-xilem remains the direct source for the tools.

- 2026-08-22, slice 14: **Knife tool.** Drag a line; a live red
  preview shows the line and the points where it crosses the outline
  (`knife_hit_points`); on release `knife_cut_glyph` splits the
  contours at those crossings. Both are core functions on
  `norad::Glyph`. Verified: a horizontal cut through "A" adds 8 points
  (20 → 28) at the crossings.

- 2026-08-22, slice 15: **text-in-scene helper + Measure tool.** Built
  `text_label::draw` (thread-local Parley contexts + Masonry's
  `render_text`) to draw shaped text into a canvas scene, the thing the
  framework should provide and doesn't. Then the Measure tool: segment
  lengths, handle lengths, stem/counter widths (`glyph_measurements`)
  and side bearings (`side_bearings`), all from core, drawn as colored
  lines with numeric labels. Verified on "A": edge lengths, three stem
  widths, and LSB/RSB (-19/-59). The text helper also unblocks grid
  cell labels (next).

- 2026-08-22, slice 16: **grid cell labels.** Each cell now shows the
  glyph name (left) and unicode codepoint (right) in a label row, via
  the text helper, matching the gpui grid. This was the last visibly
  missing overview feature.

- 2026-08-22, slice 17: **HyperPen tool.** After reading Raph Levien's
  hyperbezier post (linebender.org/blog/hyperbezier, 2026-08-08) and
  Colin Rofls's pen-tool post: hyperbezier contours carry only on-curve
  points and the curve is solved by the spline (G2-continuous by
  construction), so the pen is click-only, no handles. Wired to core's
  `start_hyper_contour` / `append_hyper_point` / `close_hyper_contour`
  (the contour is tagged with a "hyperbezier" identifier; points are
  Curve+smooth, corners are Line). Alt-click places a corner; click near
  the first point closes; a faint preview line follows the cursor.
  Verified: 5 on-curve points with no handles produce a smooth closed
  hyperbezier (core `contour_is_hyper` confirms), rendered via
  `Path::Hyper` through the spline solver. runebender-xilem's
  `tools/hyper_pen.rs` was the structural reference.

- 2026-08-22, cleanup for testing: removed the headless demo/save env
  hooks that mutated the glyph or saved on startup. Left three harmless
  read-only conveniences (`RUNEBENDER_OPEN=<glyph>`,
  `RUNEBENDER_TOOL=measure`, `RUNEBENDER_CAT=<category>`). Release build
  verified. README now has a testing guide and a keyboard reference.
  Ready for a human to run: `cargo run --release -- Font.ufo` (work on
  a copy; save writes back).

- 2026-08-22, slice 18: **UI/UX rework toward runebender-gpui.** Studied
  the gpui render tree and matched its shape: a three-column layout
  (left panel, center = title bar + canvas + status bar, right info
  panel), full-height panel columns via `sized_box` with
  `Dim::Stretch`. The editor's left column is now a vertical **tool
  icon palette** using runebender-core's `toolbar_icons()` (select,
  pen, hyperpen, shape-rectangle, shape-ellipse, knife, measure),
  drawn by a new `IconWidget` (paints the core icon path, hover/active
  states, click to switch tool), replacing the text-button toolbar.
  Added a right **info panel** (glyph name, unicode, advance, points,
  selected) and a `titlebar`. Removed the duplicated sidebar. Forced:
  - **No cross-axis stretch on flex.** `CrossAxisAlignment` has only
    Start/Center/End, so full-height columns need `sized_box` +
    `Dim::Stretch` and the background must live on the stretched box,
    not the inner content (whose height is natural). A flex `Fill`
    cross-alignment is the obvious missing thing.
  - **No icon/vector-path button.** Masonry's `svg` view takes an SVG
    string, not a kurbo path, so painting a core `ToolbarIcon` needed a
    custom widget. The design kernel should ship an icon button.
  Still to match: resizable panels, the bottom preview strip, richer
  right-panel sections (selection/transform/curves), exact panel
  colors, and the glyph-grid variable cell widths.

- 2026-08-22, slice 20: **variable-width grid cells** (ported gpui
  `glyph_column_span` + `pack_spans`: wide glyphs and long names span
  more columns, rows packed, last cell grows to fill). Plus a
  **correctness fix that is the session's biggest design signal**: the
  editor island edits its *own clone* of the session and only emits
  events, so `App` never received interactive edits, and save/preview
  read a stale glyph (headless tests missed this because they scripted
  `app.session` directly). Fixed by pulling the island's live session
  back through the element (`element.widget.session`) in `View::message`
  before the callback. This round-trips the session widget -> app ->
  widget on every edit. See DESIGN.md section 17 for what this means for
  the fork.

- 2026-08-22, slice 21: **editable advance field (panel -> document
  editing).** The info panel's Advance is now a text input; typing a
  value calls `session.set_advance` and refreshes, moving the right
  sidebearing and marking modified. This validates the DESIGN.md 17
  resolution direction: with the session synced into app state, a panel
  field can edit the document and the canvas reflects it on rebuild.
  Focus note: the field owns its buffer (initialized on glyph open, not
  re-derived each rebuild) to avoid clobbering the user's typing, since
  the app has no per-field focus awareness yet. That missing
  focus/field-binding story is the next thing the framework should
  provide (a number field bound to a value with drag-to-scrub, as in
  DESIGN.md 5's input list).

- 2026-08-22, slice 22: **Selection section (X/Y/W/H).** The right panel
  shows the selected points' bounding box (`session.selection_bounds`),
  matching gpui's Selection panel. Read-only for now; making X/Y move
  and W/H scale the selection is the next step and needs the
  document-in-app-state model from DESIGN.md 17 to be clean.

- 2026-08-22, slice 23: **Path section in the right panel.** Icon
  buttons (from core toolbar icons) for flip-h/flip-v/rotate, the four
  boolean ops, and decompose, wired through a new `App::apply_op` helper
  (clone session, run op, refresh). These ops were keyboard-only before;
  now they match gpui's panel. Confirms the panel->document path: a
  panel button mutates the app's session and the canvas reflects it on
  rebuild (the DESIGN.md 17 direction, working for whole-glyph ops).

- 2026-08-22, slice 24: **editable glyph fields (name, unicode).** The
  Glyph panel now has Name (rename on Enter via `rename_glyph`, which
  updates component references and reorders the list), Unicode
  (`set_glyph_unicode`), and Advance text inputs. Rename rebuilds the
  cache and re-points the open session. This rounds out per-glyph
  metadata editing to match gpui's glyph info panel.

- 2026-08-22, slice 25: **preview strip + a headless-tool fix.** The
  editor now has a bottom preview strip showing the current glyph
  rendered filled, via xilem's built-in `canvas` view (no custom
  widget). This surfaced a real tool bug: the `canvas` view populates
  its scene in `rebuild`, but the headless screenshot did `build` only,
  so canvas-view content was invisible in screenshots (it worked at
  runtime). Fixed in xix: `TestHarness::render_root_mut` + the
  screenshot path now runs one `rebuild`. Findings: (1) display-only
  islands can use the built-in `canvas` view with no boilerplate,
  unlike interactive ones; (2) headless render must run a rebuild to
  match what the user sees.

- 2026-08-22, slice 26: **right-click context menu in the editor**
  (Reverse, Remove Overlap, Flip H/V, Rotate 90, Decompose), painted
  inside the island's scene with hover highlight, matching how both
  other Runebender ports do it (hand-rolled, because there is no
  framework menu/overlay). This is the third place the app has had to
  build its own menu, reinforcing DESIGN.md D5: the window-level
  action + menu layer (native bar via muda, in-window via layers +
  understory_overlay) is required framework, not app code.

- 2026-08-22, slice 27: **Set Start Point + Round Corners** added to the
  context menu (core `set_contour_start`, `round_selected_corners`),
  matching more of gpui's contour context menu.

- 2026-08-23, slice 29: **window-level keymap (the behavioral half of
  menus).** Added a `ShortcutHost` widget that wraps the whole app.
  Masonry bubbles key events up the ancestor chain, so the host
  receives any key the focused widget did not consume, matches it
  against a keymap, and dispatches a typed `AppAction` (Save on Cmd+S,
  tool letters v/p/b/u/o/e/m, Escape to overview). Cmd+S now works
  regardless of focus, and the editor island's ad-hoc op keys were
  removed (ops live in the context menu and Path panel). Three harness
  tests prove dispatch works while a button is focused. The View
  wrapper follows `sized_box`'s single-child pattern (build/rebuild/
  teardown/message delegate to the child, but the host intercepts
  `AppAction` messages). This is the renderer-agnostic foundation the
  native menu bar (muda) will sit on: one `AppAction` list, driven by
  the keymap now and by menu items later. Note: undo and document
  edits stay in the island because it owns the live session; a full
  window-level Cmd+Z needs the document-in-app-state refactor (D2).

- 2026-08-23, slice 30: **Duplicate + anchor editing.** Duplicate
  selection (Cmd+D and context menu, `duplicate_selection`). Anchors
  are now editable, not just displayed: Add Anchor from the context
  menu at the right-click position, click an anchor to select it
  (highlighted), drag to move, Delete to remove. Anchor hit-testing
  takes priority over points. Matches gpui's anchor add/select/drag/
  delete.

- 2026-08-23, slice 31: **curvature comb overlay.** A Curves panel
  toggle draws the curvature comb (core `cubics_from_norad` +
  `curvature_comb`): thin strips from each on-curve sample to its outer
  point, with the outer envelope, scaled by curvature. Matches gpui's
  Curves section. Verified on "O".

- 2026-08-23, slice 32: **sidebearing editing.** In the select tool,
  dragging the advance (right) metric line changes the advance
  (`set_advance`); dragging the left line shifts the whole glyph and
  advance (`shift_glyph`), i.e. the left sidebearing. Hit-tested away
  from points. Build-verified (a live drag is not screenshottable).
  Known limitation: the panel Advance field is buffer-owned and does
  not re-derive after a drag, so it shows the old number until reopen,
  the focus/field-binding gap noted in DESIGN.md D6.

- 2026-08-23, slice 33: **sidebar sort toggle** (by name / by unicode),
  applied to the filtered cells. Matches gpui's sort control.

- 2026-08-23, slice 34: **mark colors.** A Mark section in the info
  panel with a none swatch plus the theme's mark colors
  (`theme.mark_list`); clicking sets the glyph mark
  (`set_glyph_mark`), which shows as the cell border in the overview.
  Matches gpui's mark-color feature (gpui sets it via a grid context
  menu; here it is a panel section for the open glyph).

- 2026-08-23, slice 35: **grid multi-select.** Cmd-click toggles a cell
  in a multi-selection, shift-click selects a range in cell order,
  plain click resets to one. Multi-selected cells draw with the accent
  border and a tinted background. Matches gpui's grid multi-select.

- 2026-08-23, slice 36: **apply mark to grid selection.** When cells
  are multi-selected in the overview, the right panel shows a Mark
  section that applies the chosen color to every selected glyph
  (`set_glyph_mark` per glyph, cache rebuilt). Completes the
  multi-select workflow (gpui: marks apply to the whole selection).

- 2026-08-23, slice 37: **new glyph.** When the sidebar search holds a
  name not in the font, a "+ New <name>" button appears; clicking it
  creates an empty glyph (encoded if the name is a single character),
  rebuilds the cache, and opens it. Matches gpui's add-glyph.

- 2026-08-23, slice 38: **designspace masters (loading + switching).**
  Opening a `.designspace` now loads all sources as masters (not just
  the first). A master bar in the editor lists them; clicking switches
  the active master, which reloads the glyph in that master keeping the
  viewport, and saves the previous master's in-memory edits back first.
  Verified on Newsreader VF (10 masters): the 6pt ExtraLight "A" (thin,
  advance 1588) versus the 72pt Regular. This is master editing, not
  interpolation yet; the interpolation ghost and axis sliders are the
  next step. Note per-master save writes each master's UFO
  (`master_paths`), which `save` still needs to iterate (currently
  saves the active master's source).

- 2026-08-23, slice 39: **interpolation ghosts.** In the editor, the
  current glyph's outline in every other master draws faintly behind
  the active one (`ghost_outlines`), the cross-master reference gpui
  calls the interpolation ghost. Verified on Newsreader VF: the 8 other
  masters' "A" fan out behind the ExtraLight one. This is a static
  overlay of the real masters, not computed interpolation at an
  arbitrary axis location (that needs `var_model`, a later step).


- 2026-08-23, slice 40: **real interpolation.** Moving an axis slider
  now computes a genuine interpolated instance at an arbitrary axis
  location and draws it read-only over the editable master, in warm
  amber (distinct from the green editable outline and the faint
  reference ghosts). Mechanism follows runebender-web's `interpolateGlif`
  and fontTools' model: `FontModel` stores the designspace axes (with
  their avar `<map>`) and each master's design-coord location;
  `interpolate_outline(glyph, location)` builds a per-master value vector
  (width + point coords), normalizes master locations and the target in
  **design** coordinates (user coords mapped through the axis `<map>`
  first, so avar-mapped axes interpolate correctly), runs
  `var_model::VariationModel::interpolate`, and rebuilds the outline on
  the active master's contour structure as a template. Masters stay
  editable (Glyphs 4 behavior); the slider only previews. An axes bar of
  `slider(min, max, value)` controls per axis sits under the master bar;
  when the location is off any master an "interpolated" label shows.
  Verified headless on Merriweather Sans (wght 300..800, avar-mapped to
  design 82..180): "A" at wght=650 renders an instance visibly between
  the Regular and ExtraBold masters. What the framework lacked: a
  reactive `slider` view existed (built in), but the whole
  document-drives-a-computed-overlay loop is hand-wired here — the kind
  of derived-view-state the Island/document split (D1/D2) should own.

- 2026-08-23, slice 41: **UI/UX pass toward gpui parity (labels +
  slider snap).** Two changes from comparing against runebender-gpui.
  (1) Overview cells now stack the glyph name and its `U+XXXX` on two
  left-aligned lines (name in text/mark color, unicode muted below),
  matching gpui's cell-labels box; before, a long name and the
  right-aligned codepoint overlapped on one baseline. (2) Axis sliders
  snap to the active master's location on open and on master switch
  (`master_axis_values`, design->user through the axis map), so opening
  a master shows the plain editable outline with no interpolation
  overlay until a slider actually moves (Glyphs 4 behavior). Both
  verified headless on Merriweather Sans.

- 2026-08-23, slice 42: **interpolation in the preview strip + master
  row.** The info panel gained a "Master" row (active master name) in
  the editor. The bottom preview strip now renders the interpolated
  instance in warm amber whenever the axis sliders are off a master, so
  the whole editor (canvas overlay, hint, preview) reflects the current
  axis location coherently; on a master it shows the plain glyph. This
  is the aesthetic half of the interpolation feature: the preview reads
  as "what you get at this location", like Glyphs' preview.

- 2026-08-23, slice 43: **gpui-style editor titlebar.** The titlebar
  now shows the back button, the glyph name (prominent), the source
  file name (muted) beside it, a flex spacer, and a save-status label
  ("Saved" muted / "Not saved" in warning yellow) before the Save
  button, matching runebender-gpui's header (filename title + yellow
  save status). What the framework offered: `FlexSpacer::Flex(1.0)`
  pushes the save cluster to the right edge. Before, the title read
  just "Overview"/glyph name with the Save button crowded beside it.

- 2026-08-23, slice 44: **composite-aware interpolation.** Interpolation
  now handles composite glyphs, which are most of a font. The value
  vector gained each component's x/y offset (after the contour points,
  matching runebender-web), and the rebuild resolves components: it
  writes the interpolated offsets back and recursively interpolates each
  component's base outline at the same location, transformed by the
  component affine (depth-guarded at 8 for cycle safety). Verified
  headless on Merriweather Sans "Aacute" at wght=620: both the base "A"
  and the acute accent interpolate heavier and stay correctly placed.
  Also fixed a headless-open init bug: the Name/Unicode fields now seed
  from the opened glyph, not the first glyph in the font.

- 2026-08-23, verification: **multi-axis interpolation** confirmed on
  GoogleSansCode (wght + MONO). The axes bar shows both sliders and the
  overlay is a true 2D interpolation (wght=600, MONO=1); the standard
  `VariationModel` handles N dimensions, and per-axis design-coord
  normalization composes correctly. No code change needed. Layer-based
  designspace sources load as additional masters.

- 2026-08-23, slice 45: **read-only instance swap (web/Glyphs
  behavior).** Off a master, the editor now *swaps* the outline for the
  interpolated instance and drops every editing affordance (points,
  handles, component fill, anchors, measure overlay), instead of ghosting
  an amber instance on top of the still-editable master. runebender-gpui's
  own note says the overlay approach "leaves you editing something you
  cannot see"; the web editor swaps and marks the view read-only, so this
  matches the reference. The instance draws filled + stroked in warm amber.
  Pointer edits and edit keys are swallowed while off-master (Escape still
  bubbles to leave the editor). On a master the full editable outline
  returns. Verified headless on Merriweather Sans at wght=620 (instance)
  and wght=300 (editable).

- 2026-08-23, slice 46: **tools in the header (gpui layout).** The tool
  icons moved from a left vertical palette into the editor header, laid
  out horizontally after the title (matching runebender-gpui's
  `header_tools`). The editor's left column collapses, so the canvas,
  master bar, and axes bar span the full width. `tool_palette` (the old
  vertical column) is removed. The overview still shows its category
  sidebar on the left. Verified headless: the seven tools sit in the
  header with the active one highlighted, and the canvas is wider.

- 2026-08-23, slice 47: **compact info-panel rows.** The read-only rows
  (Master, Points, Selected, Name/Unicode/Advance in overview) now put
  the label left and the value right-aligned on a full-width 18px row
  (`sized_box` stretch + `FlexSpacer::Flex`), matching gpui's panel
  layout instead of a left-hugging label/value pair.

- 2026-08-23, slice 48: **theme switching.** A header button cycles the
  four shared OKLCH themes (dark/midnight/gray/light, matching gpui);
  `cycle_theme` reloads the palette and rebuilds the baked cell colors,
  so one id swaps every role across the app (the design-token kernel in
  action). RUNEBENDER_THEME sets the initial theme for headless shots.
  Inactive sidebar category chips moved to the `control` surface so they
  stay legible in the light theme.

- 2026-08-23, slice 49: **editor glyph navigator.** The editor's left
  column now shows the glyph grid as a navigator (search + a 2-column
  grid), matching runebender-gpui, which keeps the grid visible while
  editing. Single-clicking a cell opens that glyph (`open_glyph`); the
  open glyph is highlighted. Reverses the earlier full collapse of the
  editor's left column (tools stayed in the header).

- 2026-08-23, slice 50: **richer right panel + em-box.** The Path
  section gained gpui's labeled Transformations buttons (Harmonize,
  Balance, Optimize, Round, Reverse) below the icon rows, wired to the
  existing session ops. The editor canvas now frames the glyph with a
  bounded em-box (advance width by ascender..descender) instead of
  infinite sidebearing lines, matching gpui/Glyphs. The info panel
  widened 220->250px so the master name and buttons fit.

- 2026-08-23, slice 51: **Width / LSB / RSB metrics row.** The editor
  info panel replaces the single Advance field with gpui's three-field
  metrics row: Width (set_advance), LSB (shifts the glyph so the ink
  left edge sits at the value, advance unchanged), RSB (changes the
  advance so the gap past the ink right edge equals the value). Each
  commits live; the sibling buffers refresh from `side_bearings` after a
  commit and on glyph open/switch. Values verified on "A" (1360/32/30).

- 2026-08-23, slice 52: **per-context cell size.** `CellMetrics` gained
  a `cell` field so the grid's cell edge is a parameter, not a const.
  The overview now uses spacious 104px cells (closer to gpui's roomy
  grid) while the editor navigator keeps compact 84px cells. One grid
  widget, two densities.

- 2026-08-23, slice 53: **overview Languages + Filters sidebar.** The
  category chips are now one section of a three-part sidebar matching
  runebender-gpui: Categories, Languages (script groups from
  `core::sidebar::language_groups`, each with its live glyph count), and
  Filters (GF coverage sets from `builtin_filters`, shown present/
  expected). Selection is a new `Sel` enum (Category | Language |
  Filter); `cell_matches_sel` drives `filtered_cells` via
  `glyph_matches_language_group` / `glyph_matches_character_filter`.
  Verified on Merriweather Sans: Latin 545, GF Latin Core 324/324.

- 2026-08-23, slice 54: **color the grid glyphs by mark (visual pass).**
  The overview and navigator now fill each cell's glyph with its mark
  colour (selected cells use the ring colour, unmarked glyphs the
  default fill), matching runebender-gpui, where the grid's vibrancy
  comes from the font's per-glyph marks. Before, every glyph was flat
  gray while only the border carried the mark. This is the single
  biggest visual-quality gap closed.

- 2026-08-23, slice 55: **flat sidebar tree (visual pass).** The
  Categories / Languages / Filters rows are no longer pill buttons but
  flat, full-width list rows: label left, count right-aligned, a subtle
  highlight (+ accent text) when selected, and section headers with a
  disclosure caret, matching gpui's sidebar. Built on `button(child,..)`
  wrapping a `flex_row` in a `sized_box(Dim::Stretch)`. Pain point
  logged: xix needs a first-class styled list-row primitive; this is
  hand-assembled from a button + spacer + fixed-height box.
