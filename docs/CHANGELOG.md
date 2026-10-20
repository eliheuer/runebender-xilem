# Changelog

Notable user-facing changes will be recorded here and copied into each GitHub Release's notes.
Entries under Unreleased describe work being prepared, not an available public release.

## Unreleased

### Added

- Chat: every reply is restated for the designer before it is shown. A
  second request, constrained to a schema with no tools, turns the model's
  reply into one or two plain sentences and a detail; a Detail word under
  the answer opens the rest. The model's own reply stays in the history for
  follow-up questions, and is logged beside the restated one. When the
  restating request fails, the reply is shown as it is.
- Design context in three layers, read by agents before they design: the editor's default `type-design` and `font-engineering` files in `assets/context/`, the designer's own `*.context.md` files in `~/.config/runebender/context/` (or `RUNEBENDER_CONTEXT_PATH`), and the font's own `*.context.md` files beside the designspace up to the repository root. A later layer wins where it disagrees. `design_context` returns the files on the live and disk tool surfaces; the Chat panel puts the merged text in the local model's prompt within a budget.
- The Chat panel prints every tool call, its arguments and its result to the terminal, so a wrong call can be read instead of guessed from the model's reply.
- Proposal operations `transform_points` (move or scale a chosen set of points, by index or by a region in font units, handles included) and `adjust_width` (add to the advance), so a request such as "wider by 64" is one short edit instead of a redraw.

- Local macOS application and drag-to-Applications DMG packaging for maintainer testing, with a temporary application icon.
- Live MCP tool `analyze_font`: one audit of a whole source, with counts by mark label, off-grid points, kinks, stem medians per case and missing or incompatible glyphs against every other source, worst offenders named.
- Live MCP tool `analyze_glyph`: stem and bar, ink spans, node continuity, grid fit, mark label, and point-structure compatibility with every other source, with a reason.
- The Chat panel serves a selected GGUF through a supervised `llama-server` child when that binary is found beside the editor, at `RUNEBENDER_LLAMA_SERVER`, or on PATH: loopback only, the model's own chat template for tool calls, thinking off by default, one server kept across turns and stopped with the panel. `font-ml chat` remains the fallback. A chat model folder now needs only a `.gguf`.
- Live MCP tool `flag_glyph`: the agent's needs-eyes flag. Marks a glyph pink and appends a dated reason to its UFO note; refuses green, purple and yellow glyphs and never clears a mark.
- A `triage-font` workflow, served as an MCP prompt and a skill, that runs `analyze_font` and ranks what needs the designer's eyes.
- The editor removes dead live-session sockets left by crashed or killed editors when it starts.
- `analyze_glyph` on the saved-file tool surface too (`runebender agent call analyze_glyph --font`), with compatibility against the other masters of a designspace.
- Chat panel tool set now includes `analyze_glyph`, `analyze_font`, `glyph_inventory`, `editor_context`, `read_kerning` and `specimen`, and the prompt tells the model to quote measured numbers rather than compute them from points. Typing `/review-glyph H` or another workflow name sends that workflow's text.
- `runebender mcp --install` writes the live server, by the binary's absolute path, into Claude Code, Codex, Claude Desktop and Cursor settings; `--dry-run` reports without writing.
- A `runebender` hub skill and prompt with the connection, targeting and safety rules; every skill under `skills/` declares `surface: runebender-live-v1` and the host refuses another surface.
- Live MCP workflows served as prompts (`review-glyph`, `compatibility-check`, `spacing-pass`, `kerning-pass`, `draft-weight`); the same files under `skills/` can be linked into a client's skills folder.

### Changed

- Chat: the model picker opens as a list of full-width rows, the chosen
  model inverted, in place of the chip that opened it. The conversation
  gives up height first, so the controls stay inside the panel.
- Chat: the conversation is a list that grows up from the field and shows whole cards only; a card the panel cannot show entirely is left out instead of cut by the top edge. The wheel scrolls up through older cards, and a new card keeps the list on its end.
- Chat: the cards take the theme's mark colors, as the glyph grid does: yellow for what the person typed, green for an answer, red for a problem, and a purple-to-pink wash while the model works.
- `propose_edits` on the live surface returns `changes`: for each glyph, the advance, sidebearings, ink width and point count before and after, whether the outline moved, and a warning when the new advance ends inside the outline. The Chat panel shows this account of a proposal as the editor's own note beside the model's answer, and gives each proposal task a time stamp so the same request can run twice.
- Chat: two proposal entries for one glyph are joined into one entry with both operations before the batch is written, instead of being refused as a duplicate.
- Chat: earlier turns keep only what was asked and answered; their tool calls and results no longer ride along in the next request, and the local server's context is 32768 tokens. A request that still exceeds it reads as a plain sentence.
- Chat: a glyph revision the model mistyped in `propose_edits` is replaced by the revision a tool returned for that glyph in the same run, so a 76-character hash no longer has to survive transcription by a small model. A glyph the model never read keeps the stale-revision guard.
- Chat: the local model no longer gets an experiment `branch` parameter to fill with a guess, a layer named `foreground` (or `default`, `public.default`) reads the glyph itself instead of failing, and the prompt tells the model to correct a failed call once before reporting it.

- While the chat model works, its status is the last card in the conversation, with the cards' keyline around a soft yellow-green to blue-green wash that drifts slowly across it. The wash takes the field's lightness in every theme, and the status drops its trailing ellipsis, whose glyph in the UI font read as a smudge.
- Chat errors read as one plain sentence with what to do next, such as "The conversation got longer than the model can hold. Clear the chat and ask again, or ask for a smaller step.", with the original message under it in lighter type for bug reports. One function maps every chat failure to that language. A reply cut off at the context limit now says so, where before it surfaced as a missing finish reason.
- The chat model's context is 16,384 tokens, up from 8,192.
- The working wash runs from green to blue-green.
- Hard shadows on panels, glyph tiles and graph nodes are the shape swept along the offset, not a slid copy. A copy slid down and to the left showed at the top-left corner as a crescent that thickened along the edge beside the keyline; the swept shape is an even band that follows the corner. Geometry tests check the notch is filled and the area matches the sweep.
- Outlined controls, fields and cards draw even corners. Masonry stroked a border at the full corner radius from a centerline half a stroke inside the box, so its corner arcs were not concentric with the box and a sliver of ground showed between fill and border, which read as heavier corners. The pinned xilem fork now strokes at the radius minus half the width and fills to the stroke's centerline.
- A chat turn no longer ends when a tool says no. A rejection, such as a glyph that is not in the font, goes back to the model as the tool's answer, so it can retry or say what it could not find. Before, the turn stopped and the same error printed twice.
- Tools accept a glyph by its character: `U+0035`, `uni0035`, or `5` finds `five`, on the live and disk surfaces.
- `analyze_glyph` reports point counts, total, on-curve, and off-curve, so a model answers "how many off-curve points" from a number. `font_info` reports whole font units as integers, 1024 rather than 1024.0.
- Field placeholders are the text ink at half strength in every theme, so a hint reads as a hint beside typed text. The gray theme's muted step was one shade from the text and the two looked the same.
- While the chat model works, its status ("Measuring a glyph…", "Loading the model…") is a wrapped caption above the field. It had been a bare label in the control row, and a long tool name pushed the whole left column past the dock until the work ended. A layout test now types into the chat field, rebuilds the view, and sets the working status, and fails if the column widens at any of those steps.
- Section headers (Categories, Coordinates, Glyph and the rest) show no hover or press. The hover fill sat inside the group's inset and stopped short of the rules and edges, which read as a mistake. Folding the section is the feedback, and the control tests now assert that a header paints the same in every state.
- Rail panels no longer print their name under the tab strip; the tab icon is the label. The Chat panel opens with a short note on what to do.
- The Chat panel is a column of cards that share the field's keyline: your question on the selection face, the model's answer on the panel face, and a problem on a light red wash in the normal ink. Text in every card starts on the field's text edge. The field grows a line at a time as you type, Return sends, Shift+Return starts a new line, and Escape stops the model. The hint shows only before the first question, Send is disabled while the field is blank, and Return while the model works keeps your text instead of dropping it. One spacing step sets the rhythm between cards, around the field, and to the panel edge.
- The Local AI panel's weight debt block is built from the panel row vocabulary, so its lines wrap at the dock; it had used bare labels that pushed the whole left column wider than the dock, which cut off the buttons and stretched the marks bar. The last legacy row kind is gone from the vocabulary, and a layout test now renders every rail with the marks bar at the dock width and fails on the first widget that leaves it.

- The canonical source repository is now [fontgarden/runebender](https://github.com/fontgarden/runebender).
  Existing GitHub repository links redirect to the new location.

### Known limitations

- The first public release is still in preparation; the local DMG is ad-hoc signed, and Developer ID distribution has not yet been validated.
- Windows desktop installation is not a supported release target.

Before v0.1 is published, curate the editor's initial feature list, tested platforms and source formats, installation requirements, and remaining limitations here.
See [RELEASING.md](RELEASING.md) for the release workflow and publication gates.
