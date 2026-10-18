---
name: runebender-draft-weight
description: Draft a heavier or lighter master for glyphs in the open Runebender editor from an approved source, as a proposal the designer grades. Use when asked to bolden, embolden, draft the Bold, or fill weight debt.
---

# Draft a weight

Glyphs: {{glyphs}}. From source id: {{from}}. Into source id: {{into}}.

The result is a proposal in the target source. The designer installs, nudges or discards it. Marks and small details drafted this way usually need the designer's hand; say so in the report.

1. Connect and call `project_info`. Confirm both source ids.
2. Measure the targets: `analyze_glyph` on H, O, n and o in both sources gives the stem and bar in each. The ratio between sources is the weight change. Record it.
3. For each glyph call `analyze_glyph` and `read_glyph` in the `from` source. Keep each revision. Skip any glyph whose mark label says it is not approved unless the designer says otherwise.
4. Prefer the editor's own machinery when it exists: call `graph_discover` and look for a weight or bolden node; if present, use `graph_run` on the glyph list with the `from` and `into` sources and inspect the result with `graph_image` before `graph_apply`. If no such node exists, continue with step 5.
5. Otherwise draft by offset: move each contour point outward along its normal by half the stem difference, keep point counts and types identical to the `from` glyph so the pair stays compatible, keep vertical metrics, and snap to the project grid. Widen the advance by the stem difference unless the project's rule says otherwise.
6. Create the proposal in the `into` source with `propose_edits`, task `draft-weight-<date>`, with a reason that states the stem targets. Check it with `analyze_glyph` on the proposal layer: the `sources` entry for the `from` source must be compatible and the stem must sit near the target.
7. Proof the proposal layer beside the approved Bold controls with `proof`. Report per glyph: target stem, achieved stem, points off grid, and anything that needs hand work. Install only when asked.
