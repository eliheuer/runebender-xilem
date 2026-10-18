---
name: runebender-triage-font
description: Audit a whole source in the open Runebender editor and rank which glyphs need the designer's eyes first, without changing anything. Use when asked what to fix next, where the problems are, or to review a master.
surface: runebender-live-v1
---

# Triage a font

Source id: {{source}}. Limit per list: {{limit}}.

Read only. Nothing in this workflow edits, installs, saves or changes a mark.

1. If not connected, call `editor_sessions`, then `editor_connect` with the socket the user's editor owns. Confirm the project path and note the `document_epoch`.
2. Call `project_info` and use the stable source id given above. If none was given, ask which source, or use `editor_context` to read the active one.
3. Call `analyze_font` for that source with the limit above. It returns counts by mark label, the glyphs with the most points off the 8 unit grid, glyphs with kinks or G1 curve joins, stem medians for uppercase, lowercase and other with the farthest glyphs, and for every other source the glyphs that are missing or not point compatible.
4. For the three worst names in each list, call `analyze_glyph` to see the exact numbers before naming them in the report. Glyphs listed under no vertical stem, such as W, Z and S, are not weight problems; skip them.
5. Call `proof` with the worst five glyphs and two approved neighbours so the designer can see them together. If your client does not show images, say so.
6. Call `flag_glyph` for each of the ranked glyphs, at most ten, with the one number that put it there as the reason. It marks the glyph pink with a dated note and refuses glyphs the designer has graded green, purple or yellow; report those in words instead.
7. Report in this order: the counts in one line, then the ranked list with the number for each and whether it was flagged, then what you could not verify. Do not propose edits unless asked.
