---
name: runebender-spacing-pass
description: Propose sidebearing changes for a group of glyphs in the open Runebender editor, proofed in a text line, as a proposal the designer installs or discards. Use when asked to space, fit or even out glyphs.
surface: runebender-live-v1
---

# Spacing pass

Glyphs: {{glyphs}}. Source id: {{source}}.

A proposal, never a direct edit. The designer installs it.

1. Connect, call `project_info`, and choose the source id above.
2. Read the control glyphs first: `read_glyph` for H, O, n and o in that source. Their `lsb` and `rsb` set the rhythm. Round stems from `analyze_glyph` on H and n.
3. For each target glyph call `read_glyph` and keep its `revision`. Decide the new left and right sidebearings against the controls: straight sides match H or n, round sides match O or o, open sides sit between. Keep values on the project grid, 8 units for Virtua Grotesk.
4. Proof before proposing: call `specimen` with a line that puts each target between controls, for example `HHxHH OOxOO nnxnn oxo`, and look at the image.
5. Create one proposal with `propose_edits`: a unique `task` such as `spacing-pass-<date>`, a `reason` that states the rule used, and for each glyph a `set_width` plus point moves that shift the outline by the left change. Supply the revision you read for every glyph.
6. Proof the proposal layer with `proof` using `layer` set to the proposal's layer name, in the same groups as step 4. Compare.
7. Report the old and new sidebearings per glyph in a table and the rule you used. Call `proposal_install` only after the designer says to, with `authorization` set to `user-approved`.
