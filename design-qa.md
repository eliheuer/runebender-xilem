# Design QA: Text-tool parity follow-up

final result: passed

## Inputs and state

- GPUI reference: user-supplied
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 10.59.39 AM.png`, 2442 by
  1656 device pixels at macOS 2x density. Text tool, Gray theme, text `123`,
  active sort `one`, proof strip visible.
- Earlier Xilem implementation: user-supplied
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 10.58.51 AM.png`, 2250 by
  1478 device pixels at macOS 2x density. Text tool, Gray theme, seeded text
  `1`, active sort `one`, proof strip visible.
- Behavioral references: `runebender-gpui/src/edit/input.rs`,
  `runebender-gpui/src/view/canvas/editor.rs`, and
  `runebender-web/src/Runebender.vue`. GPUI, Web, and Masonry's editable text
  widget all insert ordinary logical character keys directly and reserve IME
  commits for composed text.
- Rendering references: GPUI `paint_sort_boxes` and `paint_text_caret`, plus
  Web `draw_text_buffer`, `append_text_sort_metric_box`, and
  `append_text_sort_corner_marks`.
- Post-fix Xilem captures use a 1221 by 828 logical viewport at density 2,
  producing the same 2442 by 1656 device-pixel frame as the GPUI reference.

## Rendered implementation

- `docs/visual-audit/2026-09-14/149-xilem-text-parity-gray-2x.png`: Gray,
  `one`, Text tool, `123`, active sort `one`.
- `docs/visual-audit/2026-09-14/150-xilem-text-parity-light-2x.png`: the same
  state in Light.

## Comparison history

- Resolved P1, behavior/accessibility: Xilem consumed `Key::Character` while
  waiting for an IME commit that ordinary macOS typing did not deliver to this
  custom canvas. It now inserts character keys directly, reports the complete
  Text buffer, and retains the separate IME preedit/commit path. The harness
  covers direct typing, composition, deletion, arrows, multiline navigation,
  selection, copy, cut, and paste.
- Resolved P1, layout/behavior: the Text paint branch returned before the
  floating glyph metrics card. It now keeps the active `one / 0031 / 40 / 384
  / 88 / one` card visible in both post-fix captures.
- Resolved P1, layout/color: the active-only rectangle was replaced with the
  GPUI/Web model: full quiet metric boxes for inactive sorts plus clipped
  corner marks at the descender, baseline, ascender, and sort top. The full
  boxes also include x-height and cap-height when distinct.
- Resolved P2, icon/shape: the bare caret rule is now a full sort-height rule
  with inward triangular caps whose size follows the on-screen sort height.
- Resolved P2, content/state: the live Text buffer supplies the canvas, active
  tab title, and proof strip. All three surfaces show `123` in the matched-size
  captures.
- Resolved P2, spacing: Text fitting now uses Web's full sort bounds, leaves
  horizontal breathing room, and reserves the floating card. The top/bottom
  metric frame and caret no longer clip or run behind the card.

## Fidelity surfaces

- Fonts and typography: the three outlines and proof use Virtua Grotesk's
  source paths; no substitute text font or fake glyph art was introduced.
- Spacing and layout: source and final Gray frames were compared at identical
  device dimensions and density normalization. The run fits above the metrics
  card with intact top and bottom bounds.
- Colors and tokens: glyph ink, quiet metric, metric guide, mark header, card,
  and cursor all use the existing theme roles; Gray and Light were inspected.
- Image and icon fidelity: no raster or generated assets are involved. Metric
  marks and caret caps are native Kurbo/Masonry geometry ported from the
  established renderer behavior.
- Copy and content: `123`, active sort `one`, U+0031, and its spacing metrics
  remain consistent across canvas, tab, card, rail selection, and proof.

## Edit-rail divider follow-up

- Reference: user-supplied
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 11.13.26 AM.png`, a focused
  678 by 256 device-pixel crop of the lower-left edit rail.
- Resolved P2, stroke: the glyph-count/slider row and the marks bar both painted
  their shared boundary, producing two adjacent hairlines above the swatches.
  The status row now paints only its upper keyline, leaving the marks bar as the
  sole owner of the shared divider.
- `docs/visual-audit/2026-09-14/151-xilem-edit-rail-single-divider-gray-2x.png`:
  full post-fix Gray capture at 2442 by 1656 device pixels.
- `docs/visual-audit/2026-09-14/152-xilem-edit-rail-single-divider-focus.png`:
  focused post-fix comparison crop; the swatch boundary is one hairline across
  the rail.

## Nodes chrome parity follow-up

- GPUI target: user-supplied
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 11.16.13 AM.png`, 2538 by
  1700 device pixels. Nodes mode, Gray theme, three graph tabs, Masters open,
  and the selected glyph outline visible.
- Earlier Xilem state: user-supplied
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 11.14.24 AM.png`, 2334 by
  1502 device pixels. Nodes mode, Gray theme, mixed rounded and square toolbar
  controls, a tall footer, and no glyph preview.
- Matched post-fix implementation:
  `docs/visual-audit/2026-09-14/157-xilem-nodes-chrome-parity-gray-matched-2x.png`,
  2538 by 1700 device pixels from a 1269 by 850 logical viewport at 2x.
  Graph contents differ from the target fixture, so the direct comparison is
  scoped to the top rails, footer, and inspector composition.
- Independent theme check:
  `docs/visual-audit/2026-09-14/156-xilem-nodes-chrome-parity-light-2x.png`,
  2334 by 1502 device pixels from a 1167 by 751 logical viewport at 2x.
- Resolved P2, controls/copy: graph tabs and New, Open, Save, and Run now use
  one square, dark-keylined control recipe. `Open…` is now `Open` as requested.
- Resolved P2, alignment: the center toolbar occupies the same 36-pixel rail as
  the left tabs and owns one bottom keyline. Its controls share one baseline.
- Resolved P2, density: the node footer is fixed to the same 28-pixel height as
  the swatch bar, with a compact 20-pixel Fit graph button.
- Resolved P1, inspector structure: Nodes now keeps the overview document
  inspector, with Masters open by default and the selected-glyph outline in a
  resizable lower pane. The initial split accounts for the visible master rows
  instead of clipping them.
- Resolved P2, master rows: the selector follows GPUI's compact 20-pixel row,
  neutral selected fill, keyline, and selected-content ink rather than the
  oversized stock-button treatment.
- Fonts and typography: node chrome continues to use the shared 13-pixel UI
  type; labels and selected state differ by theme token, not size.
- Spacing and layout: matched-size Gray comparison confirms the toolbar rail,
  footer, master list, and preview remain aligned at the target viewport.
- Colors and tokens: all new borders, fills, and labels use palette roles; Gray
  and Light were inspected together.
- Image and icon fidelity: no raster assets or substitute icons were added; the
  preview is the existing live outline renderer.
- Copy and content: command labels are New, Open, Save, Run, and Fit graph; the
  status line retains the graph name, node count, link count, and live note.

## Button-shape system follow-up

- User-reported source state:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 11.55.23 AM.png`, a 636 by 852
  device-pixel crop showing rounded title tabs and a rounded selected master
  beside square node controls.
- GPUI reference:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 11.16.13 AM.png`, 2538 by 1700
  device pixels, where the comparable title tabs, graph controls, and master
  selection use square keylines.
- Post-fix implementation:
  `docs/visual-audit/2026-09-14/159-xilem-button-shape-system-gray-final-2x.png`,
  2538 by 1700 device pixels from the same 1269 by 850 logical viewport at 2x.
- Resolved P2, control drift: `ButtonShape` is now the semantic source of truth
  for square and circular controls. The Runebender `recipes::button` factory
  applies the square shape before any call-site styling, so panels cannot
  silently inherit Xilem's rounded stock button.
- Resolved P2, visible mismatch: title tabs, graph tabs, graph commands,
  inspector master rows, list rows, and compact action chips are square.
  Circular opt-in remains limited to the title-bar add control, coordinate
  dots, and colour swatches.
- Fonts and typography: no type metrics changed; the shared 13-pixel UI type
  remains consistent across each control family.
- Spacing and layout: the shape migration changes no control dimensions,
  padding, alignment, or panel split.
- Colors and tokens: existing palette roles and keyline colors are unchanged.
- Image and icon fidelity: no image or icon assets changed.
- Copy and content: no labels changed in this follow-up.
- Focused comparison was necessary because the reported mismatch is confined
  to the title strip and Masters section; the full final capture confirms that
  the rule also holds across the node toolbar and footer.

## Text-sort context follow-up

- User-reported source state:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 12.22.58 PM.png` and
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 12.23.06 PM.png`. The first
  shows `Render` in Text mode; after activating `R`, the second loses the run
  and shows only the edited glyph.
- Behavioral references: GPUI `activate_sort_at_pos` changes the active edit
  session without reseeding `edit_buffer`; Web
  `loadActiveTextSortGlyphIntoEditor` reloads with `metricsOnly: true` and
  `seedTextBuffer: false`.
- Resolved P1, state: sort activation now replaces the current tab's glyph
  session without invoking ordinary glyph/tab navigation. The tab identity,
  parked text context, full word, proof strip, and Text tool remain intact,
  even when the activated glyph already has another tab.
- Resolved P2, color: Web maps `metricGuide` to the theme accent. Xilem's
  clipped sort intersections now use that green accent while complete inactive
  metric boxes retain the quieter neutral role.
- Post-fix captures:
  `docs/visual-audit/2026-09-14/160-xilem-text-sort-context-gray-2x.png` and
  `docs/visual-audit/2026-09-14/161-xilem-text-sort-context-light-2x.png`, both
  2442 by 1656 device pixels. Both show the full `Render` run, active `R`
  metrics, the `Render` proof, and green intersection ticks.
- Static captures verify the intact rendered state and both theme recipes. The
  state-transition regression test verifies that activating a sort keeps the
  text tab and its word; native pointer delivery remains outside headless QA.

## Validation

- `cargo test --offline --bin runebender` (142 passed, 4 ignored)
- `cargo test --offline --test cli` (17 passed)
- `cargo test --offline --test live_agent` (passed with local Unix-socket
  permission)
- `cargo clippy --offline --workspace --all-targets -- -D warnings`
- `cargo fmt --check`
- `git diff --check`
- Matched-size Gray comparisons and independent Light captures visually
  inspected for both Text and Nodes modes

## Text-to-Select composition follow-up

- User-reported reference:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 12.39.59 PM.png`, showing GPUI
  with `test` retained while Select exposes the first `t`'s outline and nodes.
- Behavioral references: GPUI keeps one `edit_buffer` across tool changes and
  translates the editable glyph through `EditorState::sort_offset`; Web keeps
  `has_text_session` separate from `text_mode_active` and uses the active
  sort's layout origin for both drawing and hit testing.
- Resolved P1, composition lifetime: each Xilem editor tab now records whether
  it owns an open text composition independently of its active tool. Selecting
  an outline tool no longer removes the composed run, tab text, or proof.
- Resolved P1, in-context editing: Select, Pen, HyperPen, shape, metrics,
  component, anchor, point, and marquee geometry now share the active sort's
  translated glyph coordinate system. The glyph's editable nodes sit at the
  same position as the omitted active text fill, while neighbouring sorts stay
  visible.
- Resolved P2, mode chrome: the caret and text selection appear only in Text;
  in Select the active sort becomes editable outline chrome and its metrics
  card remains visible, matching the GPUI screenshot.
- Post-fix captures:
  `docs/visual-audit/2026-09-14/162-xilem-text-select-context-gray-2x.png` and
  `docs/visual-audit/2026-09-14/163-xilem-text-select-context-light-2x.png`, both
  2438 by 1612 device pixels. Both show Select active, the full `test` run, the
  first `t` editable in place, and `test` retained in the proof strip.
- The state-transition regression verifies Text to Select to Text preserves
  the tab's word; the geometry regression verifies drawing and hit conversion
  share the active sort origin. Static captures verify both palette recipes;
  native pointer and IME delivery remain outside headless visual QA.

## Start-point and anchor parity follow-up

- Source visual truth:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 4.43.37 PM.png`, 1020 by 914
  pixels, GPUI Gray editor focused on the `a` outline.
- Earlier implementation:
  `docs/visual-audit/2026-09-14/170-xilem-start-anchor-before-gray-2x.png`,
  1221 by 828 pixels. Start nodes remained ordinary circles or squares and a
  second blue arrow sat beside each one; unselected anchors had an unrelated
  dark center.
- Post-fix implementation:
  `docs/visual-audit/2026-09-14/172-xilem-start-anchor-close-gray-2x.png` and
  `docs/visual-audit/2026-09-14/173-xilem-start-anchor-close-light-2x.png`,
  both 1221 by 828 pixels at a 1221 by 828 logical viewport and 0.8 editor
  zoom. The focused Gray canvas was cropped to 608 by 632 and normalized to
  914 pixels high for comparison.
- Combined comparison evidence:
  `docs/visual-audit/2026-09-14/174-gpui-xilem-start-anchor-comparison.png`,
  1900 by 914 pixels, with the GPUI source on the left and normalized Xilem
  crop on the right.

### Comparison history

- Resolved P2, start-point semantics: the detached arrow and ordinary start
  node were replaced by GPUI's single directional wedge at the exact node
  position. Corner starts use a crisp triangle; smooth starts round all three
  corners. Fill, ring, halo, selection, traversal order, and scale continue to
  come from the ordinary point recipe.
- Resolved P2, anchor style: unselected anchors now use solid theme pink inside
  the same dark point keyline, rather than a pink ring around a dark core.
  Selected anchors retain the shared selected-point treatment.
- Post-fix comparison found no remaining actionable P0, P1, or P2 mismatch in
  the requested start-point, direction-indicator, and anchor styling scope.

### Fidelity surfaces

- Fonts and typography: this marker-only change adds no text and changes no
  type treatment.
- Spacing and layout rhythm: marker centers remain on the exact design points;
  no panel, glyph, or metric layout moved. The focused crop was normalized only
  for visual comparison.
- Colors and visual tokens: direction wedges inherit their point kind and
  selection roles; anchors use the existing pink mark and point-outline roles.
  Gray and Light were inspected.
- Image quality and asset fidelity: the markers remain native vector geometry;
  no raster assets, generated substitutes, or approximate icons were added.
- Copy and content: no interface copy or font data changed.

final result: passed

## Curvature-overlay layering follow-up

- Supplied implementation baseline:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 5.06.15 PM.png`, 2250 by
  1460 pixels, showing Xilem with the comb painted over handle lines and point
  markers and an opaque edit-outline fill.
- Supplied visual reference:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 5.06.55 PM.png`, 2400 by
  1594 pixels, showing GPUI with the comb beneath the outline, handles, and
  points and the design grid visible through the filled outline.
- Behavioral source reference: GPUI `paint_scene` paints
  `paint_curvature_comb`, then `paint_outline`, `paint_handles`, and
  `paint_points`; its `outline_fill` recipe multiplies the shared role alpha
  by 0.70. Xilem now follows that same layer order and named fill recipe.
- Post-fix captures:
  `docs/visual-audit/2026-09-14/175-xilem-comb-layering-gray-2x.png`
  and
  `docs/visual-audit/2026-09-14/176-xilem-comb-layering-light-2x.png` show the
  standard fitted editor state;
  `docs/visual-audit/2026-09-14/177-xilem-comb-layering-close-gray-2x.png`
  and
  `docs/visual-audit/2026-09-14/178-xilem-comb-layering-close-light-2x.png`,
  both 2250 by 1460 device pixels at a 1125 by 730 logical viewport and 1.5
  editor zoom.
- Combined comparison evidence:
  `docs/visual-audit/2026-09-14/179-gpui-xilem-comb-layering-comparison.png`,
  2533 by 1065 pixels, with the GPUI reference crop on the left and Xilem
  implementation crop on the right.

### Comparison history

- Resolved P1, editing visibility: the curvature strip is now painted before
  the glyph outline, handle lines, off-curve circles, on-curve nodes, anchors,
  and continuity rings. All editing targets retain a clean uninterrupted edge
  over the comb.
- Resolved P2, canvas depth: the `outlineFill` theme role now passes through a
  single `Palette::outline_fill` recipe at GPUI's 70% opacity. The filled glyph
  remains legible while the design-grid dots and metric rules remain visible
  through it.
- The comb colors and opacity were intentionally left unchanged; the supplied
  GPUI reference uses a vivid opaque comb and obtains clarity from paint order.
- Post-fix comparison found no remaining actionable P0, P1, or P2 mismatch in
  the requested curvature-comb stacking and outline-transparency scope.

### Fidelity surfaces

- Fonts and typography: this canvas-only change adds no text and changes no
  type treatment.
- Spacing and layout rhythm: no panel, glyph, metric, or control geometry
  moved; the comparison crops normalize only the visible canvas region.
- Colors and visual tokens: outline fill uses the existing shared
  `outlineFill` role through one named opacity recipe. Comb, point, handle,
  anchor, and continuity colors remain theme-driven. Gray and Light were
  inspected.
- Image quality and asset fidelity: all affected marks remain native vector
  geometry; no raster assets or generated substitutes were added.
- Copy and content: no interface copy or font data changed.

### Validation

- `cargo test --workspace -- --test-threads=1` with the two sandbox-only
  Unix-socket tests skipped (153 application tests, 17 CLI tests, 352 core
  tests, and 5 mark-feature tests passed; 4 model tests remained ignored)
- `cargo clippy --workspace --all-targets -- -A
  clippy::allow-attributes-without-reason -D warnings`
- `cargo fmt --check`
- `git diff --check`
- Gray and Light standard-fit captures, close-up captures, and the direct
  GPUI/Xilem comparison visually inspected

final result: passed

## Point-marker design-grid windows

- Supplied GPUI reference:
  `/Users/eli/Desktop/Screenshot 2026-09-14 at 7.10.28 PM.png`, 2262 by
  1496 pixels, showing design-grid dots preserved inside point markers.
- Behavioral source reference: GPUI `paint_points` fills each point, redraws
  the portion of the active dot or line grid inside it using the point hue,
  then paints the point ring. Xilem now uses the same three-layer recipe.
- Post-fix captures:
  `docs/visual-audit/2026-09-14/180-xilem-point-grid-window-gray-2x.png`
  and
  `docs/visual-audit/2026-09-14/181-xilem-point-grid-window-light-2x.png`,
  both 2250 by 1460 device pixels at a 1125 by 730 logical viewport and 4.0
  editor zoom. The centered and displaced grid dots remain visible inside
  point interiors in both themes.
- Additional glyph proof:
  `docs/visual-audit/2026-09-14/182-xilem-point-grid-window-close-gray-2x.png`
  exercises the same point treatment on `eight` at 2.0 editor zoom.
- Combined comparison evidence:
  `docs/visual-audit/2026-09-14/183-gpui-xilem-point-grid-window-comparison.png`,
  1600 by 900 pixels, with the supplied GPUI crop on the left and Xilem Gray
  on the right.

### Comparison history

- Resolved P1, alignment legibility: the point interior no longer masks an
  underlying design-grid intersection. A centered dot now remains centered
  and an off-grid dot remains visibly displaced inside the marker.
- Resolved P2, marker hierarchy: the grid fragment is painted after the point
  interior but before its outline, so the point ring remains uninterrupted.
- Resolved P2, grid-mode parity: dot mode redraws round grid dots and line
  mode redraws clipped vertical and horizontal chords, matching GPUI.
- Post-fix comparison found no remaining actionable P0, P1, or P2 mismatch in
  the requested point-marker grid-visibility scope.

### Fidelity surfaces

- Fonts and typography: this canvas-only change adds no text and changes no
  type treatment.
- Spacing and layout rhythm: no panel, metric, control, or glyph geometry
  moved.
- Colors and visual tokens: embedded grid fragments use the point's existing
  theme hue, or its outline color in filled-point themes, with the same
  zoom-dependent opacity as the canvas grid. Gray and Light were inspected.
- Image quality and asset fidelity: point windows and grid marks remain native
  vector geometry; no raster assets or generated substitutes were added.
- Copy and content: no interface copy or font data changed.

### Validation

- Focused point-window tests cover centered and displaced dot intersections,
  clipped line-grid chords, and GPUI's coarse/fine zoom thresholds.
- `cargo test --workspace -- --test-threads=1` with the two sandbox-only
  Unix-socket tests skipped (157 application tests, 17 CLI tests, 352 core
  tests, and 5 mark-feature tests passed; 4 model tests remained ignored)
- `cargo clippy --workspace --all-targets -- -A
  clippy::allow-attributes-without-reason -D warnings`
- `cargo fmt --check`
- `git diff --check`
- Gray and Light two-times headless captures and the direct GPUI/Xilem
  comparison were visually inspected.

final result: passed
