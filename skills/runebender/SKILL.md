---
name: runebender
description: Work on the font open in the Runebender editor through its live MCP tools. Start here for connection, document targeting and the safety rules, then use a focused runebender-* skill for the task.
surface: runebender-live-v1
---

# Runebender

The live tools talk to the unsaved document inside the running editor. Nothing here reads or writes font files on disk, and nothing saves.

## Connect

1. Call `editor_sessions`. Each entry is one open editor window with its socket, project path and age.
2. Call `editor_connect` with the socket of the window the designer means. If more than one editor is open, ask which font before connecting. A closed endpoint never reconnects on its own; connect again.
3. Call `project_info`. Note the `document_epoch` and the stable source ids. Display order can change when sources are reordered; the id does not. Use the id in every later call.
4. Call `editor_context` to learn the active source, layer and glyph when the request says "this glyph" or "the current master".

## Rules

- Read before you judge. `analyze_glyph` returns stems, bars, kinks, grid fit, the mark label and compatibility with the other sources. `read_glyph` returns points. `proof` and `specimen` return images. State a number only after a tool returned it.
- Edits are proposals. `propose_edits` writes a proposal layer the designer installs, nudges or discards in the editor. Do not call `proposal_install` or `proposal_discard` unless the designer asked for that exact glyph in this conversation.
- `agent_apply` edits the live glyph and needs the designer's grant, carried as `user_approved` with the current revision. Reuse the same actor, operation key and payload after a lost response, or read `agent_receipt`. `agent_cancel` with the same identity stops a pending apply; it never undoes a committed edit.
- Mark labels are the designer's grading. Never set green, purple or yellow, and never clear a mark. `flag_glyph` is the one mark an agent may set: pink, with a dated one-line reason written into the glyph's note, on glyphs that are red, orange, blue, pink or unmarked.
- Never save, export or write into the font's folder. The designer saves.
- When a request is ambiguous about which source, glyph or layer, ask. One question is cheaper than one wrong edit.

## Which skill

- `runebender-triage-font`: audit a whole source and rank what needs eyes first. Read only.
- `runebender-review-glyph`: look at, check, critique or measure one glyph. Read only.
- `runebender-compatibility-check`: whether glyphs interpolate between sources, and the first reason they do not.
- `runebender-spacing-pass`: sidebearings for a set of glyphs, as proposals.
- `runebender-kerning-pass`: kerning for pairs or a line of text, in an experiment fork.
- `runebender-draft-weight`: draft a heavier or lighter master from an approved source, as proposals.

Each skill is also served by the live MCP host as a prompt of the same name without the `runebender-` prefix.
