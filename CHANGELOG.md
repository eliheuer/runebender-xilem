# Changelog

All notable changes to runebender-xilem. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

- Align category sidebar insets and full-width rules; group font counts under Filters.

- Share working Glyphs, Axes, and Local AI navigation across font, node, and glyph workspaces.

- Match GPUI thumbnail ink centering, em-relative scale, and compact caption spacing; keep tall marks inside their cells.

- Fit glyph rows to the viewport, give rail thumbnails GPUI proportions, and propagate grid size/theme changes on rebuild.

- Give word proofs the GPUI initial drawing height and start additional inspector sections folded.

- Add a contour-selection Shapes rail, inverted word proofs, connected coordinate reference controls, and GPUI-style glyph and node shadows.

- Add editable shaped word previews with cached Vello CPU blur, larger outline previews, shared rail search controls, and clearer selected-state and metrics styling.

- Add selection width/height editing around the chosen reference point and record inspector undo immediately.

- Align inspector section borders, spacing, ordering, and path-operation groups with GPUI; fit overview tiles across the available width.

- Keep GPUI live-node files out of the disk runner; experiments remain available through MCP.


No releases yet. `AGENTS.md` has the checklist for the first one.
Until then, `main` is the only line and this section stays open.

### Changed

- Align the Xilem header, editor rail, and node canvas with GPUI's shared visual hierarchy.

- Shared live experiment, kerning and drawing tools through MCP; refresh open sessions after application and preserve undo. Uses the core Designbot proof interface.

- Core pin updated for revision-checked agent proposals. Installing a guarded
  proposal skips glyphs whose foreground changed after the proposal was made.

- Themes: Midnight removed, Gray is the default. Dark, Gray, Light.
- Undo lives in core. `Session` holds an `EditHistory` from
  `runebender_core::document::history` instead of its own `UndoState`,
  and records, undoes, and discards through it.
