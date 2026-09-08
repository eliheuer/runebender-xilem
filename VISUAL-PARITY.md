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
