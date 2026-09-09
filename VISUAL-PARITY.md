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
