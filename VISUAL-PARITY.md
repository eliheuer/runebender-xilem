> Current execution checklist: [PARITY-WORKLIST.md](PARITY-WORKLIST.md), reviewed September 11, 2026. This document retains earlier scope and evidence.

<!-- Copyright 2026 the Runebender Authors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

# Visual parity

This is the visual companion to `GPUI-PARITY.md`. It records only what
has been compared or rendered; source similarity is not a claim of pixel
parity.

## Reference provenance

- GPUI reference commit: `c52842dedbd34e4f7027c8e539fba5053881e917`
  (`Keep completed proof snapshots until rerun`).
- GPUI had uncommitted UI work when this pass began. The relevant changed
  surfaces were `src/view/chrome.rs`, `src/view/panels/editor_sidebar.rs`,
  `src/view/panels/tabs.rs`, `src/view/theme.rs`, canvas, preview, grid,
  render, and input files. That work was read only and was not reset,
  stashed, or changed.
- Xilem baseline: `d3919ca` (`Pin live proof controls in core`), isolated
  branch `codex/xilem-visual-parity`.

## Checked pass: editor rail and header

The Xilem editor rail now uses the same three-surface hierarchy as the
current GPUI rail: titlebar-colour recess, intermediate inactive tab, and
the selected tab's panel surface. It uses the shared theme's `titlebar`,
`panel`, `controlSelected`, and `controlSelectedInk` tokens rather than
hard-coded colours. The Xilem header now uses the GPUI header's darkened
selected-control surface; active tools use the same control-selection
tokens.

The overview inspector now opens in the GPUI reference's compact accordion
state: the common font sections remain available as headers, Masters stays
open, and the existing keyboard/pointer-accessible section toggles retain
their behavior. Header ink follows the shared selected-control ink in Gray
and Light, while Dark retains its ordinary readable text ink.

The editor title bar also retains the font file name beside its save state,
matching the GPUI edit view instead of substituting the glyph tab for the
document identity. This was checked at the 1100x720 compact viewport.

Only functional Xilem rail entries are rendered: Glyphs, Axes when the
font has axes, and Local AI. GPUI's Shapes and Chat entries are not
imitated until they have actual Xilem panels and accessible actions.

## GPUI screenshot reference: 2026-09-07

The owner supplied three current GPUI captures, retained outside this
repository at:

- `/Users/eli/Desktop/Screenshot 2026-09-07 at 8.59.40 PM.png` — Font
  overview: charcoal header, dense category/sidebar rows, bordered mark
  cells, full-width accordion inspector, and the status/zoom strip.
- `/Users/eli/Desktop/Screenshot 2026-09-07 at 9.00.02 PM.png` — Glyph
  editor: compact icon rail, dotted editing canvas, flat metric popover,
  preview, and dense inspector controls.
- `/Users/eli/Desktop/Screenshot 2026-09-07 at 9.01.00 PM.png` — Nodes:
  filled alignment dots, flat marked node headers, outlined ports and
  coloured keyed wires.

Those captures are Retina images; the Xilem unattended renderer is fixed
at scale 1.0. Compare layout relationships, token contrast, and control
density across them rather than treating physical screenshot pixels as
logical layout sizes.

### Nodes canvas

The Xilem node canvas now fills its shared core grid circles, producing the
same quiet solid-dot field visible in the GPUI node capture. It retains
core's 16-unit snap geometry and Xilem's Vello paths; node state and graph
semantics are unchanged.

## Render evidence

Matched input: `VirtuaGrotesk.designspace`, glyph `n`, 1100x720 logical
pixels, display scale 1.0, no live window. Captured with:

```sh
RUNEBENDER_SCREENSHOT=/private/tmp/xilem-rail-gray.png \
RUNEBENDER_THEME=gray RUNEBENDER_GLYPH=n RUNEBENDER_SIZE=1100x720 \
cargo run --quiet -- /Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk.designspace
```

The same command with `RUNEBENDER_THEME=light` wrote
`/private/tmp/xilem-rail-light.png`. Both images were visually inspected.
The renderer is CPU/Vello and does not establish GPU, native-window,
pointer, keyboard, IME, or screen-reader parity. A fresh GPUI screenshot
was intentionally not captured: doing so would require opening/managing a
GUI while the owner is using the machine. The committed GPUI source and
its known dirty state are the reference for this pass.

The current nodes check used `bolden.nodes.json` with
`RUNEBENDER_NODES=/Users/eli/GH/repos/virtua-grotesk/nodes/bolden.nodes.json`
and `RUNEBENDER_MODE=nodes`, producing
`/private/tmp/xilem-nodes-gray.png` at 1920x1298 logical pixels. It was
visually inspected against the supplied Nodes capture. It is a structural
comparison only: graph layout and file contents differ, and it does not
assert GPU or interaction parity.

## Remaining gaps

- Compare matching GPUI pixels once an unattended GPUI capture command is
  available; the supplied captures now cover the overview, editor, and
  nodes surfaces.
- Bring the header tab faces, grid/list filters, inspector density,
  preview/splitters, nodes, and proposal surfaces through the same
  screenshot-led loop.
- Functional prerequisites remain for Shapes, Chat, native input/IME, and
  some node/Local-AI interactions. They remain owned by the functional
  parity work, not this visual branch.
The header workspace tabs now use the GPUI treatment: restrained keylines on
the header ground, with the selected Font, Nodes, or glyph tab marked in the
warning role rather than an inverted pill. This was rendered in Gray at
1100x720 after the change.

The editor metrics card follows the GPUI placement and hierarchy: it is
centered over the bottom of the canvas, keeps the glyph mark in its header,
and uses GPUI's compact 288px two-row layout: editable LSB, width, and RSB
fields first, then the two kerning groups. The hit targets are derived from
the same rectangles the painter uses.

The editor rail now gives its compact glyph grid the full rail width and a
five-column density, matching the current live GPUI web frame at 1352×864.
The overview grid retains its independent, user-controlled cell size.

The overview grid now uses GPUI's separate caption band below each thumbnail,
with its 1352×864 live-frame inset and eight-column geometry. Compact rail
thumbnails retain their shorter, caption-free rows.

Linked node ports use the same typed mark color as their wire, rather than the
generic node ink; the source, model, glyph, and layer connections therefore
read as continuous colored paths.

## Screenshot-led follow-up, 2026-09-08

Compared native GPUI at 79e3ab1 with Xilem rendered at 1200x800 on
Virtua Grotesk glyph n. Corrected the compact rail: fill available column
width, reduce excessive glyph margins, and suppress captions on spanning
rail tiles (including Arabic names). Painting and hit testing share the
expanded cell geometry. Shared fields and action buttons now have square,
keylined edges. Tightened the coordinate picker and corrected metrics-card
header contrast and borders. Gray and Light screenshots inspected.

Validation: cargo build --locked, cargo fmt --check, git diff --check,
and all 33 tests passed. GPUI native screenshot inspected; Xilem final
proofs are Masonry CPU renders. Native capture stalled and the temporary
Xilem app wrapper opened without a font, so native final parity is unverified.

Remaining visible differences: inspector section borders/order and full-width
control layout; header and rail tool inventory; metrics-card field layout;
preview content and controls. These changes do not establish full parity.

## Inspector and overview follow-up, 2026-09-09

The user's GPUI overview and R-editor screenshots are the visual reference.
Added a shared inspector-group recipe with full-width dividing rules and
consistent insets. Coordinates and transformations precede Glyph; path
operations now fold independently. Operation rows stretch across the panel;
curve labels match the reference. Overview tiles distribute remaining width
instead of leaving an unused strip. No framework fork or dependency change.

### What actually prevents parity

No rendering blocker was demonstrated for these changes. Masonry already
provides layout, borders, padding, text, and custom widgets. The immediate
work is in this application: W/H coordinate editing, matching tool inventory,
preview text/blur controls, selection colors, and consistent metrics fields.
These must preserve real behavior; absent controls must not be painted as
working features. Arabic shaping and local-AI correctness need separate
workflow verification and cannot be established by these screenshots.

Framework development costs remain: composing polished reusable controls,
bridging custom widgets to views, and keeping complex generic view types
manageable. Contribution candidates are an inspector/form example, reusable
control recipes, and reproducible screenshot fixtures. These are opportunities,
not evidence that the desired appearance is impossible. The old XILEM-GAPS
document explicitly describes historical findings and must not be presented
as a current blocker list.

Proofs use the existing Masonry CPU screenshot path. They verify layout and
paint, not native input, GPU rendering, or full visual parity.

### Workday kickoff: coordinate dimensions

Implemented W/H fields beside X/Y. Resize selected points about the chosen
quadrant using the existing core transform, rejecting nonfinite, nonpositive,
and degenerate dimensions. Fixed inspector operations to drain pending undo
snapshots immediately; undo now refreshes coordinate values. Regression test
uses a disposable UFO and verifies reference preservation, invalid input, and
undo. All 34 tests pass; the selected-R screenshot verifies four-field layout.
Next: reference picker connecting lines, header/rail tool layout, preview
text/blur controls, and selected-state theme alignment.

### Sustained visual implementation: rail and proof surfaces

- Shared real search/scope/regex/case controls between overview and glyph rail.
  Rail tabs stretch across the column; the search row uses compact insets.
- Larger toolbar icon artwork and GPUI's yellow selected-content ink in the
  grid, list, and sidebar; ordinary tool selection remains separate.
- Editable word proofs via the existing TextInputs/TextState/core text engine.
  Latin Runebender and Arabic salaam screenshots inspected; this does not
  certify complete Arabic shaping or interactive IME support. Direction
  controls live beside the proof. Off-master locations retain the interpolated
  current-glyph proof and label it explicitly.
- Real preview blur slider, cached Vello CPU raster, composited as an image.
  Pinned imaging_vello and imaging_vello_hybrid 0.0.1 reject group filters;
  imaging_vello_cpu 0.0.1 supports them. This is a concrete backend capability
  gap, not a reason to redesign the application or claim all Vello backends
  support blur. No dependencies added. The CPU image-compositing regression
  test passes; native GPU interaction/compositing still needs live verification.
  The last raster is cached by paths, transform, dimensions, sigma, and color;
  raster dimensions are bounded without cropping large views.
- Overview preview now shows outline, control handles, and point types at a
  useful size instead of a small filled glyph.
- Metrics card reduced to one row with readable centered numbers. Kerning
  group labels remain read-only and are abbreviated to avoid overlaps; editing
  those groups still belongs to the existing inspector controls.

Validation: 35 tests, Clippy with warnings denied, and docs passed before the
final metrics text-centering adjustment; final build and rendered proof check
follow that adjustment. Proofs in /private/tmp/xilem-blur-gray.png,
xilem-blur-light.png, xilem-progress-overview.png (and copied into this task's
visualizations folder). No font sources, GPUI files, or dependency pins changed.
Remaining: full rail panel inventory, closer header/tool layout, connecting
lines in the reference picker, complete editable metrics groups, node surfaces,
and native interaction proof.

## September 9 follow-up: shapes and proof surfaces

The editor has a Shapes rail listing contours and their point counts. Clicking
a contour selects its points and refreshes the coordinate fields. Component
rows are informational: component selection and transforms remain a gap.
The reference picker retains nine real buttons with connecting rules.
Word proofs now offer inversion using shared selected-surface theme colors.
Glyph tiles and node cards use lower-left shadows; selected node headers use
the selected-control surface.

Headless CPU renders cover the Gray editor, Shapes rail, and node graph.
These are visual layout proofs, not native GPU or pointer-interaction proof.
The GPU backend's group-filter limitation remains worked around by the
cached Linebender CPU blur image; native compositing still needs a GUI check.

## September 9 proportions pass

GPUI `src/view/render.rs` assigns the initial preview panel 140 logical
pixels. Xilem previously assigned 120 to both drawing and controls. The
drawing now receives 140 plus a separate control-row allocation. Dock widths
are named together to prevent drift. Path Operations, Background, Color, and
Masters start folded, matching the supplied editor screenshot; their controls
remain available. The Color header retains the existing Mark state key.

Gray and Light 1200x890 headless renders were inspected in
`/private/tmp/xilem-proportions-gray.png` and
`/private/tmp/xilem-proportions-light.png`. Build, Clippy with warnings denied,
and all 35 tests passed. These are approximate content dimensions, not an
exact pixel comparison: reference screenshots have different window sizes and
canvas zoom. Remaining visible gaps include shorter rail cells, smaller canvas
glyph framing, extra proof controls, missing in-canvas text context, and the
inspector's Measure section versus the GPUI Axes/Shaping inventory.

## September 9 grid sizing and rebuild pass

Read GPUI `src/view/grid.rs`: rail rows use a 1.18 aspect factor, and
row heights divide the available viewport. Xilem now uses those rules in
its existing grid widget. Grid rebuilds now propagate changed cell metrics
and palette pointers; previously they updated selection and cell data only.
Changing metrics resets scroll so a stale offset cannot hide resized rows.

Inspected 1200x890 headless Gray editor and Light overview images:
`/private/tmp/xilem-grid-fit-gray.png` and
`/private/tmp/xilem-grid-fit-light.png`. Build, Clippy with warnings denied,
and 35 tests pass. Whole-row fitting applies at the initial scroll position;
GPUI's row-quantized wheel scrolling is still a behavioral gap. Overview
caption/thumbnail proportions and ink fitting need another comparison pass.
GPUI and Xilem both use a 0.62 initial canvas-fit factor: the much larger R
in the reference is a different zoom state, not evidence for changing that
default arbitrarily.

## September 9 thumbnail transform pass

GPUI's `cell_glyph_transform` in `src/view/grid.rs` uses 0.65 em fill,
0.92 thumbnail fill, visible-ink centering, and an em window expanded for
tall marks. Xilem now follows the same placement calculations, replacing
its ascender/descender fit. Caption padding is 5 pixels, leading is 1.10,
and the second line starts at the GPUI 90-pixel base-column threshold.
Spanning glyphs use base-column caption policy. Text drawing now receives
line centers rather than baseline coordinates.

Inspected Gray and Light overview renders at 1200x890:
`/private/tmp/xilem-ink-gray.png` and `/private/tmp/xilem-ink-light.png`.
The enlarged/clipped-looking glyphs in the previous proof are corrected.
Build, Clippy with warnings denied, and 36 tests passed, including a new
thumbnail regression covering ink centering, period scale, and tall marks.
Remaining overview gaps include the missing rail tab strip, sidebar group
spacing and borders, bottom control styling, and matching zoom/selection
state for an exact reference comparison. No upstream Xilem blocker was
needed to explain or fix these thumbnail differences.

## September 9 shared rail navigation

Overview and node workspaces now use the same navigation-strip builder as
the glyph editor. Glyphs opens categories, Axes opens the existing axis
controls when axes exist, and Local AI opens the existing models/tasks/nodes
panel. Shapes remains editor-only; returning from Shapes to overview shows
categories with the Glyphs tab active. No inert Chat tab was added.

Inspected Gray overview and Light Local AI overview renders at 1200x890:
`/private/tmp/xilem-nav-gray.png` and `/private/tmp/xilem-nav-ai-light.png`.
Build, Clippy with warnings denied, and 36 tests pass. Models were listed
only; no task, graph, install, or font-source write was executed. Native
pointer navigation remains unverified. Remaining rail gaps include GPUI's
Chat destination, sidebar group rules/insets, and count/control placement.

## September 9 sidebar grouping pass

Removed nested Panel/Card insets from category navigation. Search and each
accordion now own one compact inset, while group rules span the full sidebar.
Exporting and incompatible-master counts are at the head of Filters and
collapse with that section. Removed the unused Card region token.

Inspected Gray and Light 1200x890 renders at
`/private/tmp/xilem-sidebar-gray.png` and
`/private/tmp/xilem-sidebar-light.png`. Build, Clippy with warnings denied,
and 36 tests pass. Remaining differences include row typography, missing
Chat navigation, native slider styling, and matching reference selection
and scroll state. No font sources or GPUI files changed.

## September 9 neutral controls

Zoom, glyph-size, blur, axis, and AI-strength sliders now share a recipe
using Masonry TrackColor and ThumbColor properties. Both track segments
use neutral theme ink; the thumb uses the panel surface. The existing
Masonry slider retains its keyboard, pointer, and accessibility behavior.
Header active tabs use neutral header ink instead of warning orange.

Gray and Light editor proofs inspected at 1200x890:
`/private/tmp/xilem-controls-gray.png` and
`/private/tmp/xilem-controls-light.png`. Those images precede a final
inactive-header contrast correction using translucent header ink. Build
and Clippy pass; 36 tests passed for the slider implementation before
the final header color adjustments. Native interaction remains unverified.
This styling required no custom widget or upstream change.

## September 9 node foreground and initial framing

Matched GPUI's foreground ordering: card bodies first, then connection
wires, then all ports. The node toolbar uses compact spacing. Initial
viewport framing now fits graph bounds with a margin, capped at 100%,
rather than clipping the rightmost nodes at a fixed zoom. Authored graph
positions are unchanged; subsequent user pan/zoom is preserved.

Inspected Gray and Light 1200x890 renders at
`/private/tmp/xilem-nodes-fit-gray.png` and
`/private/tmp/xilem-nodes-fit-light.png`. All six bolden graph nodes are
visible. Build, Clippy with warnings denied, and 36 tests pass. No graph
was run or saved. Remaining gaps include file-dialog behavior, node zoom
controls in the bottom bar, exact graph arrangement/reference state, and
native pointer verification.

## September 9 node viewport controls

The Nodes bottom bar no longer displays glyph creation/list/grid-size
controls. It offers Fit graph, which requests layout and reapplies the
existing bounds fit. Opening or creating a graph also requests a fit;
ordinary edits and pan/zoom do not. This is an intentional functional
correction to the reference layout: controls must affect their workspace.

Inspected Gray and Light 1200x890 renders at
`/private/tmp/xilem-node-controls-gray.png` and
`/private/tmp/xilem-node-controls-light.png`. Build, Clippy with warnings
denied, and 36 tests pass. Pointer activation remains unverified; the
request travels through Workspace state and the canvas rebuild/layout path.
No graph execution, graph save, or font-source writes occurred.

## September 9 curvature-comb parity and integration check

GPUI reference checkout verified clean at 79e3ab1. Xilem's comb now retains
core CombSample curvature values, uses GPUI's 16 samples and normalized
74-unit maximum height, and paints outlined quadrilateral teeth through
the green/blue/purple/pink/orange theme-mark ramp. The old fixed-scale
monochrome line comb is replaced.

Inspected Gray and Light R comb renders at 1200x890:
`/private/tmp/xilem-comb-gray.png` and `/private/tmp/xilem-comb-light.png`.
Debug build, Clippy with warnings denied, and 36 tests pass. Formatting
and docs passed for the integration baseline. Release build passed for
334a5fb before the comb implementation; it does not yet include this comb
change. Remaining prominent editor gaps: continuity rings still use filled
classification dots, persistent word context is limited to the Text tool,
and reference zoom/state has not been matched for a pixel-level comparison.

## September 9 continuity overlay

Matched GPUI's `paint_continuity_rings`: non-corner classifications from
core receive a green ring at 8.55 screen pixels, with a 3-pixel point-outline
understroke and 1.5-pixel color stroke. Corners retain their ordinary point
markers. The previous filled classification dots no longer cover the points.

Inspected combined comb/continuity Gray and Light R renders at 1200x890:
`/private/tmp/xilem-curves-gray.png` and `/private/tmp/xilem-curves-light.png`.
Build, Clippy with warnings denied, and 36 tests pass. This matches overlay
geometry and color policy; overall editor parity still needs persistent
word context, matching zoom/state, remaining inspector controls, and native
interaction/rendering proof. The release binary predates this overlay pass.

## September 9 text-buffer retention

Switching away from Text previously set the canvas TextState to None,
discarding typed content. The live editor widget now parks that buffer
and refreshes it when Text is selected again. Text painting, caret clicks,
and typing are explicitly gated on Tool::Text, so a parked buffer cannot
consume outline-editing input. Scope is tool switches in the live widget,
not persistence across editor destruction, application restarts, or sessions.

Build, Clippy with warnings denied, and 36 existing tests pass. A later headless event regression covers tool-mode input gating; full
view-rebuild and native proof remain incomplete.
This does not complete persistent in-canvas word context: that requires
placing the editable glyph at its shaped sort origin and applying the same
origin to point hit testing, overlays, and metrics, while leaving shaping
and Arabic joining in core. Simply keeping the text paint branch active
would return before drawing/editing the outline and is not a valid fix.

## September 9 parked-text event regression

Added a disposable in-memory A/B text fixture and a Masonry TestHarness
keyboard test. With the same live widget, a character event in Select does
not change the parked buffer; Text accepts it; Select again preserves the
two-character buffer. Clippy with warnings denied and all 37 tests pass.
This verifies event gating, not the Xilem view-rebuild transition itself,
native IME, Arabic joining, or cross-session text persistence. No visual
rendering changed in this test-only pass.

## September 9 overview counts and caption containment

The status count now includes the primary selection, adding it only when
not already present in the multi-selection set. Long caption text is
clipped inside its glyph tile with a balanced imaging clip scope; spanning
cells retain their full width.

Inspected Mark-category Gray and Light 1200x890 renders at
`/private/tmp/xilem-marks-gray.png` and `/private/tmp/xilem-marks-light.png`.
Build, Clippy with warnings denied, and 37 tests pass. The selected A stays
selected when filtered out, so the count remains one while 84 marks are
shown; the right preview intentionally continues to show that selection.
No font marks or sources were changed. Full selection-toggle behavior and
caption ellipsis/tooltips remain separate work.

## Focused upper-left review — September 9 evening

Removed the extra titlebar sidebar icon. Category rows now use the GPUI source dimensions: 19 logical pixels high, 14 pixels horizontal padding, and no gaps between rows. Build and all-target Clippy pass. Gray and Light headless 1200x800 images were inspected; native titlebar placement remains to be reviewed in the user launch.

This section is not at full parity: the navigation strip still needs GPUI active-tab geometry and icon sizing; the fourth GPUI tab opens Chat, which needs real functionality rather than a placeholder. Search-field sizing and section-header rhythm also remain for the next focused comparison. Automation remains paused.

### Navigation tab styling

Matched GPUI 32/28px active/inactive heights, 4px rail insets and gaps, 18px icons, muted inactive ink, and top-only selected corners with an open bottom. Uses the existing Masonry icon widget because the stock radius property is uniform. Build and all-target Clippy pass; Gray and Light headless renders inspected. Three tabs remain: Chat functionality is still absent, so widths differ from the four-tab GPUI reference.

### Tab strip follow-up

Replaced the white gridBorder header separator with the panel outline token. Added the whole-strip border behind the tabs, with the selected face covering its baseline. Restored the fourth text-icon Chat tab; it honestly reports that chat is not connected, rather than submitting requests. Gray/Light 1200x800 headless captures inspected and build/all-target Clippy passed. Chat backend parity remains outstanding.

### Hidden scrollbars

Application defaults give Masonry ScrollBar widgets zero dimensions while retaining Portal scroll handling. Applies to native and headless launches and all existing panels. Gray and Light screenshots inspected. Build and all-target Clippy pass; hidden_bars_preserve_wheel_scrolling verifies wheel input moves an overflowing Portal and renders afterward. Native trackpad input was not exercised.

### UI typography

Verified bundled Virtua Grotesk TTF SHA-256 matches GPUI exactly. Label helper now specifies 13px; text_input helper now specifies Virtua Grotesk and 13px instead of SystemUi and Masonry default 15px. Build and all-target Clippy pass; Gray/Light headless renders with VirtuaGrotesk.designspace inspected. This aligns family and nominal size; field baseline/clipping and native rasterization still need matched-scale review before claiming complete typography parity.
