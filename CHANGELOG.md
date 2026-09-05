# Changelog

All notable changes to runebender-core. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

No releases yet. `AGENTS.md` has the checklist for the first one.
Until then, `main` is the only line and this section stays open.

### Added

- Live experiment forks with independent glyph proposals and kerning, selective
  conflict-checked application, and transaction undo. Session-only in this MVP.
- Designbot raw PNG/PDF scene transport, harfrust live Latin specimens, and explicit
  MCP proof export. Replaces the unshipped resvg proof renderer.

- Explicit contour drawing and smooth-flag proposal operations, live mark/Unicode
  inventory, numeric curve-join inspection, design-context documentation links,
  and explicit live proposal installation with structure policy and undo.
- PNG image content in MCP proof responses, rasterized in the CLI process.

- Native live-document agent tools over a private Unix socket, `sessions` discovery,
  and CLI/MCP `--session` routing. Reads include unsaved changes; proposals remain
  in memory for editor installation and undo.
- Stable MCP `--live` mode with editor discovery and explicit connection, plus
  project OMP configuration for existing chat sessions.

- Revision-checked agent edit batches: `project_info`, explicit master indices,
  `propose_edits` for width, point, translation, and anchor edits, and
  `agent call --args-file` (including stdin). New proposals record design intent
  and reject stale foreground glyphs at installation. Proposal creation writes
  only the new layer and layer index.
- Proposal-layer proofs through `proof --layer` and agent tools, including SVG
  content, glyph labels, and component resolution against the proposal overlay.
- AI type-design architecture and Counterpunch research, plus a thin Python client example.
- Agent node workflows reject foreground-writing nodes before execution.

- `runebender-core features [--write]`: `mark` and `mkmk` features
  written from anchors, one lookup per anchor name, with mark classes
  and filtering sets; composites without anchors take their
  components' anchors. The editor shapes with them, so a mark typed
  after a base sits on its anchor, and the text buffer lays out the
  offsets shaping gives. Node `core.features`.

- `runebender-core mcp --font <designspace>`: an MCP server over stdio
  with the agent tool list, one to one. Tools may write proposal layers; installation is separate.

- `document::history`: the one undo pile. `Master` owns an
  `EditHistory`, one stack per glyph name, with `record_undo`,
  `amend_undo`, `discard_last_undo`, `undo`, and `redo`. Shells push
  and pop here and hold no snapshots of their own.
- `document::proposal`: an edit offered but not made. A tool writes
  glyphs into a UFO layer named `com.runebender.proposal.<task>`;
  `Master::install_proposal` copies them over the foreground as one
  undo step per glyph, skipping any that break point structure when
  asked, and `discard_proposal` drops the layer. Result and error
  types derive serde and a JSON Schema (`schemars`).
- The command line grows the agent surface: `info` (names, metrics,
  counts, proposals waiting), `proof` (an SVG sheet with metric lines,
  and lsb/rsb/bounds per glyph as JSON), `proposal list|install|discard`,
  and `propose <task>`, which runs `font-ml` as a separate process and
  reports the proposal layer it left. font-ml is found through
  `--tool`, `$RUNEBENDER_FONT_ML`, or PATH; a missing tool exits 3,
  matching font-ml's own "not built" code. `tests/cli.rs` drives the
  binary end to end.

### Changed

- Themes: Midnight is gone. Dark, Gray (the default), and Light
  remain, so the token system settles on three before more are added.
- Every theme fills its glyph-grid marks the way Gray does, with a
  keyline and a dark ink: Dark at the bright step, Light at the base
  step. The grid keeps one character across themes.
- `cellSelectedFill` and `cellSelectedInk` role tokens: how a
  selected glyph-grid cell is drawn. Gray and Light invert, Dark lifts.
- A `fieldOutline` surface token in every theme: the quiet rule
  around a text field.
- Two new role tokens in every theme: `outlineFill`, the mid tone
  under the glyph in the editing view, and `metricsLine`, a quiet
  neutral rule for baseline, x-height, and the em box, which used to
  borrow the accent hue.

### Removed

- `info`. It counted glyphs, layers and codepoints, which is not a
  question worth a command.

- `measure` and `check`. `measure` reported what the editor shows and
  fontTools prints in a line. `check` compared point signatures across
  two masters, which `fonttools varLib.interpolatable` does more
  thoroughly across a whole designspace.
- `color` and `spacing`, with `analysis::optical` and
  `analysis::spacing`. Both produced lists a designer had to ignore:
  `color` flagged glyphs drawn denser on purpose, and `spacing`
  reported every sidebearing against a grid it inferred from a font
  that has none.

### Changed

- The command line is behind a `cli` feature, on by default. Editors
  depending on this crate with `default-features = false` no longer
  build clap.

