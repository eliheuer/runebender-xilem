---
name: runebender-compatibility-check
description: Check which glyphs in the open Runebender editor will not interpolate between sources, and say exactly why. Use before a variable build or after drafting a second master.
---

# Compatibility check

Glyphs: {{glyphs}}. Reference source id: {{source}}.

Read only.

1. Connect with `editor_sessions` and `editor_connect` if needed. Call `project_info` to list the sources.
2. For each glyph, call `analyze_glyph` with the reference source. The `sources` array names every other source and gives `compatible` with a one sentence `reason`: shape counts, point counts, the first point whose type differs, a component reference, or anchors.
3. Use `glyph_inventory` when the list says all glyphs or a mark label, and page with `offset` and `limit`.
4. Group the result: missing in a source, different contour count, different point count, point type mismatch, component mismatch, anchor mismatch. Give the glyph names in each group.
5. For a point type mismatch, `read_glyph` both sources and name the point index and coordinates so the designer can go straight to it.
6. Report the groups, biggest first, and stop. Fixing is a separate step the designer asks for.
