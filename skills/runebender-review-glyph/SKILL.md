---
name: runebender-review-glyph
description: Review one glyph in the open Runebender editor with numbers and a proof image, without changing it. Use when asked to look at, check, critique or measure a glyph.
---

# Review a glyph

Glyph: {{glyph}}. Source id: {{source}}.

Read only. Nothing in this workflow edits, installs or saves.

1. If not connected, call `editor_sessions`, then `editor_connect` with the socket the user's editor owns. Confirm the project path and note the `document_epoch`.
2. Call `project_info` and use the stable source id given above. If none was given, ask which source, or use `editor_context` to read the active one.
3. Call `analyze_glyph` for the glyph. Note the stem, bar, kinks, G1 curve joins, how many points miss the 8 unit grid, the mark label, and whether the other sources are point compatible.
4. Call `read_glyph` only when a number from step 3 needs its exact points.
5. Call `proof` with the glyph and two or three approved neighbours from the same source, for example the glyph beside H, O, n and o. Look at the image. If your client does not show images, say so and stop visual claims.
6. Compare against the project's `DESIGN.md` with your file tools when it is available. The designer's rules win over general taste.
7. Report in this order: what is wrong with a number attached to each point, what is fine, and what you could not verify. Name the source and layer in every finding. Do not propose edits unless asked.
