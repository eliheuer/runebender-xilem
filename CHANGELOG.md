# Changelog

All notable user-facing changes to Runebender are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project will use
[Semantic Versioning](https://semver.org/) once releases begin.

No release has been published yet.

## [Unreleased]

### Added

- Added live Rust compilation for shaped variable-font previews and TTF export, including unsaved edits, variable kerning and mark positioning.
  Desktop preview compiles in the background and discards stale revisions.
  Shared feature edits follow the default source and drafts are checked against all masters and axes.
  Browser export downloads the compiled font.
- Added source creation by interpolation, renaming, location changes, reordering, removal, and structural undo/redo in the Masters panel.
  Source removal retains its UFO directory; auxiliary glyph layers can be copied and removed.
- Added `runebender compile SOURCE --out FONT.ttf` for headless compilation without rewriting source files.

- Added a native Xilem font editor and a browser build that reuse the same Masonry widget
  tree. Browser edits use a bundled font and remain in memory.
- Added outline, component, anchor, metric, kerning/group, metadata, multi-master, and proof
  editing workflows with shared Undo/Redo.
- Added editable metaball sources, cubic conversion, and whole-font conversion.
- Added Python Babelfont package import with multiple sources, mapped axes, instances, intermediate and auxiliary layers.
  Saving writes new UFO/Designspace files and preserves the original package; unsupported metadata fails explicitly.
- Added a bidi-aware text tool with Arabic shaping, IME composition, clipboard actions,
  keyboard selection, OpenType feature and language controls, and per-tab state.
- Added Nodes workflows for font operations, local-model tasks, independent experiment
  branches, Designbot PNG/PDF proofs, guarded application, and transaction undo.
- Added reviewable local-AI proposals and local GGUF chat; model results never change
  foreground glyphs without explicit installation.
- Added live-document MCP tools for reads, proofs, proposals, kerning, and experiment
  workflows, plus shared project configuration for OMP, Claude Code, Pi with an adapter,
  and ChatGPT desktop/Codex.
- Added `runebender info`, `proof`, `agent`, and `mcp` headless commands.
- Added Linux and macOS CI, Windows build/render/native-startup diagnostics, and
  reproducible headless visual captures in Gray and Light.

### Changed

- Made Project own variable glyphs and their layers, with guarded UFO projections for existing editing tools and shared history.
- Unified mapped-axis conversion and glyph-local interpolation behind Runebender-owned APIs using pinned Babelfont and fontdrasil adapters.
  Interpolation preserves fractional advances and kerning and varies anchors and component transforms.
- Consolidated the font engine, command line, and Xilem application into one `runebender`
  package and executable.
- Grouped Xilem runtime code under `src/application`, moved named editor tools into
  `src/application/editor/tools`, and added a contributor-facing architecture map.
- Reworked the editor's menus, title bar, sidebars, inspectors, glyph grid, proof strip,
  Nodes canvas, resizing, and theme roles around shared Linebender-style design tokens and
  GPUI behavior references.
- Centralized document history so outline, metric, metadata, kerning, proposal, and
  experiment changes use the same guarded Undo/Redo model.
- Improved browser rendering for Retina and fractional scales, WASM SIMD, native cursors,
  focus, paste, composition, splitters, and idle repaint behavior.
- Reduced themes to Dark, Gray (default), and Light.

### Fixed

- Kept off-grid curve handles anchored to their captured drag-start positions during repeated snapped pointer updates.
- Matched ordinary quadratic hit testing to drawn implied joins and all-off-curve contours.
- Made line-to-curve conversion produce one cubic for zero-control quadratic segments.
- Prevented extreme finite coordinates from committing nonfinite points during segment insertion.
- Rejected stale implied-quadratic segment identities after contour topology changes.
- Preserved neighboring quadratic segments and control metadata when deleting one control.
- Made multi-contour point deletion atomic when a later contour cannot be edited.
- Preserved point identities and metadata when reversing open, closed and implied contours.
- Avoided recording no-op reversals for symmetric two-control contours.
- Preserved point and contour identities when changing a closed contour's start point.
- Reject unsupported Designspace fields, invalid mappings, missing sources and incompatible glyph structures before they can be silently dropped or misinterpreted.
- Prevented external reloads, stale model results, or stale proposals from overwriting
  unsaved or subsequently edited document state.
- Made glyph-name, Unicode, mark-color, metric, and multi-master edits atomic and
  recoverable through Undo/Redo.
- Corrected text shaping and caret behavior across mixed Latin/Arabic text, ligatures,
  combining marks, kerning, line breaks, and direction changes.
- Stabilized grid scrolling and keyboard navigation, panel and proof resizing, menu focus
  and shortcuts, and editor-tool focus transitions.

Known platform and workflow limits are tracked in
[`docs/known-limitations.md`](docs/known-limitations.md).
