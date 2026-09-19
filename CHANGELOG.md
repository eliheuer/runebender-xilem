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

- Python Babelfont package imports now construct canonical single- and multi-source documents before deriving temporary UFO compatibility projections.
- Built interpolated sources as atomic canonical document transactions, preserving exact metadata and undo while assigning fresh object identities to the new source.
- Moved Designspace structure, font information, component alignment, mark color, metrics keys and formulas, metaball payloads, and HOI intermediate points into typed canonical document storage with guarded edits and UFO boundary projection.
- Made semantic glyph-mark edits update or clear the Runebender label and public UFO color atomically while preserving exact source color spelling on a no-op.
- Moved quadratic, cubic, hyperbezier, corner-rounding, handle-harmonizing, handle-balancing, and handle-optimization operations onto canonical contours.
- Moved hyperbezier pen creation, point appending, and closure onto typed canonical contours with stable object identities.
- Added atomic canonical contour import for SVG append and image-trace replacement without whole-glyph reconciliation.
- Moved background send, swap and clear into canonical auxiliary-layer transactions with guarded source-history undo and redo.
- Moved ordinary pen creation, segment appending, and closure onto typed canonical contours with stable object identities.
- Moved explicit mask baking onto canonical contours and cleared the persisted mask key only after successful subtraction.
- Moved selected and whole-layer metaball collapse onto staged canonical layer edits that retain live groups until conversion succeeds.
- Made image placement install validated source resources through Project without mutable UFO-font access.
- New fonts and in-memory UFO imports now enter through common canonical Project constructors.
  The browser-compatible UFO boundary preserves its supported metadata and default-layer glyphs while rejecting extra layers, images, data and unsafe or inconsistent paths instead of silently dropping them.
- Replaced long-lived full UFO persistence templates with explicit glyph-free source-format records while preserving layer order and paths, residual metadata, images, data and opaque payloads.
- Composition now writes complete revision-checked proposal plans atomically and keeps the foreground unchanged until explicit installation.
- Moved live proposals and experimental versions onto stable source-identified canonical layers with revision-checked installation and Project-owned undo.
- Moved source groups and exact fractional kerning into stable source-identified canonical metadata with atomic edits and UFO boundary rehydration.
- Made Project own variable glyphs and their layers, with guarded UFO projections for existing editing tools and shared history.
- Unified mapped-axis conversion and glyph-local interpolation behind Runebender-owned APIs using pinned Babelfont and fontdrasil adapters.
  Interpolation preserves fractional advances and kerning and varies anchors and component transforms.
- Switched interpolation inputs and compiled group/kerning snapshots to canonical document values, preserving layer paint order and checking numeric quantization.
- Consolidated the font engine, command line, and Xilem application into one `runebender`
  package and executable.
- Grouped Xilem runtime code under `src/application`, moved named editor tools into
  `src/application/editor/tools`, and added a contributor-facing architecture map.
- Reworked the editor's menus, title bar, sidebars, inspectors, glyph grid, proof strip,
  Nodes canvas, resizing, and theme roles around shared Linebender-style design tokens and
  GPUI behavior references.
- Centralized document history so outline, metric, metadata, kerning, proposal, and
  experiment changes use the same guarded Undo/Redo model.
- Added atomic whole-source metadata snapshots for guarded history across source reordering.
- Improved browser rendering for Retina and fractional scales, WASM SIMD, native cursors,
  focus, paste, composition, splitters, and idle repaint behavior.
- Reduced themes to Dark, Gray (default), and Light.

### Fixed

- Made Save As publish and retarget complete UFO/Designspace copies atomically, including external relative feature includes, without replacing original or pre-existing destination files.
- Made Glyphs conversion warnings fail explicitly and publish validated generated sources without replacing an existing output directory.
- Prevented compiled-font imports from selecting an existing UFO destination and rejected aliased save destinations before staging.
- Made UFO and Designspace saves validate every staged source before replacing live files, while retaining metainfo, layer order and directories, exact GLIF paths, images, data and unrecognized filesystem payloads.
- Made sparse-source and instance edits canonical and restored instance projections across structural undo and redo.
- Rendered canonical hyperbeziers, live metaballs, smart-component poles, and nested full-affine components through Project layer paths without hidden UFO glyph reconstruction.
- Invalidated metric-dependent previews and compilation when metadata history is replayed.
- Prevented handle harmonizing and balancing from treating implied quadratic chains as cubic segments.
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
- Preserved contour metadata and surviving point identities when opening or closing paths.
- Prevented contour opening from saving orphaned cubic or quadratic controls that cannot be reopened.
- Rejected malformed imported contours and duplicate object identifiers before mutation, while preserving component interleaving during contour replacement.
- Added canonical contour copy, paste and offset duplication with fresh stable identities.
- Moved boolean and overlap-removal topology replacement onto canonical paths.
- Applied successful empty boolean results instead of retaining the original contours.
- Moved knife preview and slicing onto canonical contours while retaining untouched contour metadata.
- Preserved mixed cubic/quadratic and all-off-curve geometry in canonical knife input and output.
- Preserved implied joins across one-control and multi-control quadratic chains in knife operations.
- Moved path cleanup, direction correction, handle fitting and extrema insertion onto canonical contours.
- Moved learned and model-predicted embolden operations onto canonical points.
- Moved nested component decomposition onto canonical layers with safe copied metadata.
- Preserved editable hyperbezier contours through copy, duplicate and repeated component decomposition.
- Moved stroke expansion, offset, extrusion and roughening onto canonical contour transactions.
- Added stale-safe canonical layer snapshots for undo and redo migration.
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
