# Changelog

All notable changes to runebender-core. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

No releases yet. `AGENTS.md` has the checklist for the first one.
Until then, `main` is the only line and this section stays open.

### Added

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

