# Changelog

All notable user-facing changes to Runebender are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The project will use
[Semantic Versioning](https://semver.org/) once releases begin.

No release has been published yet.

## [Unreleased]

### Changed

- Selected metaball support-ring keylines now reuse GlyphGrid's orange mark color.

- Native live-editor sessions no longer rebuild the entire widget tree while their mailbox is idle.
  Background polling now runs only while a live request or comparison job needs the application thread.

- Reorganized the reusable font engine into `font`, `automation`, and `workflows` domains.
  Canonical font ownership remains in `font`; agent and live-editor contracts now live in `automation`; saved and live node graphs now live in `workflows`.

- Grouped compiler, persistence, and format-metadata adapters under their respective boundaries.
  Native Nodes interaction now lives in one `application/editor/tools/nodes/` directory.

### Added

- Added live `editor_open_glyph` navigation for agent clients.
  It switches the current editor tab to a named glyph while preserving text, preview, source and tool context, without editing or saving the font.

- Added the native Python recipe runtime and script-file library foundation, plus anchor report and move examples.
  Recipes receive immutable captures and return validated reports or proposals; applying an edit remains a separate guarded action.
  Process deadlines, bounded result capture and observed file-conflict checks cover the runtime boundary.

- Added full-family proof-input derivation for unpublished guarded edits.
  Changed specimen inputs preserve other masters, axes and features while the original document and baseline remain unchanged.

- Added a native Scripts workflow for ordinary Python recipe files.
  Chat can offer a completed Python artifact without saving or executing it; users can edit and revision-save it, bind validated JSON parameters to an explicit glyph/source scope, run it in a bounded background worker, review its report and guarded proposal, and explicitly Apply through ordinary editor history.
  Script execution remains unavailable in the browser.

- Added native Nodes content children for multiline Python editing and immutable PNG specimen comparison.
  Code editing keeps focus, selection, clipboard, multiline input and bounded local Undo/Redo without automatic execution; proof nodes retain prior images while running and support independent pan, zoom and presentation-only resizing.
  Live comparisons expose their source and glyph scope, support guarded topology edits plus Run, Cancel, Clear and Apply controls, and reuse retained proof bytes by immutable artifact identity.
  Comparison files preserve authoring intent without live handles or authority; Open binds an explicitly named source and never runs the graph.

- Added asynchronous native `proof_start`, `proof_status`, `proof_cancel` and `proof_release` tools.
  Completed proofs return compiled PNG images through MCP with immutable font hashes, captured revisions and explicit stale-result labels.
  Proof jobs share a bounded process-wide worker and release their document-scoped artifacts explicitly.

- Atomic edit receipts now include stable IDs of the actual changed widths, points and anchors, excluding no-ops and preserving those IDs across retries and undo.
  The procedural client harness now checks receipt-backed edits, authorization, stale writes, retries and both targeted and ordinary undo.

- Added `agent serve --font PATH --glyph NAME` for bounded headless native editor sessions on real fonts.
  Scripts and MCP clients share live reads, atomic edits, receipts and ordinary undo/redo; this host never saves source files.
  A procedural Python spacing example prepares guarded requests before explicitly applying, reconciling or undoing them.

- Added native live `agent_apply`, `agent_receipt` and `agent_history` tools with guarded width/point/anchor batches, bounded retry receipts, and shared ordinary/targeted undo.
  Exact retries do not repeat edits, view refreshes or history entries, including after a lost response or undo.
  Independent cancellation distinguishes cancelled-before-commit, too-late and committed outcomes; durable receipt recovery remains unsupported.

- Live context revisions now include the document epoch, preventing identical reopened state from reusing an earlier context token.

- Added bounded in-memory operation receipts and a native background queue for immutable compiled proofs.
  The receipt layer and proof queue are connected to native live-session adapters.

- Added bounded canonical edit transactions with grouped history, immutable compiled-proof primitives, and a disposable live-client conformance harness.
  The transaction engine backs receipt-based live tools, and completed compiled proofs can be delivered as MCP images.
  CLI-generated live prompts now share MCP's source, authorization and session guidance.

- Added a disposable `agent fixture` process for testing live clients against real application state and ordinary editor undo/redo without opening a window.

- Added native live `editor_context`, session-scoped object IDs in glyph reads, and document epoch guards for external agents.
  Live tools now advertise stable source IDs, and MCP input and protocol negotiation are bounded.
  See the [live context contract](docs/agent-live-context.md) and [compiled proof contract](docs/agent-compiled-proofs.md).

- Documented the proposed [live agent-editing architecture](docs/agent-interface-research.md), client connection matrix, source audit, and gated acceptance plan.
  This is research only; it adds no runtime capability.
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

- Simplified the Metaballs panel to whole-glyph conversion actions, adding conversion to editable hyperbezier contours.
- Metaball centers now remain visible and can be selected, multi-selected, moved, nudged and deleted with the Select tool using the standard outline-point selection treatment.
- Simplified the native Metaballs panel with quieter, evenly inset sliders, integer-style readouts and standard panel actions.
- Completed the canonical editing-model cutover by removing persistent Norad source/glyph projections, mutable source guards and legacy source-local history.
  Norad now remains behind reviewed source-format and proposal codecs, enforced by an item-aware architecture test.
- Headless font information, SVG proofs and proposal list/install/discard now read and edit the canonical Project document and save through the shared persistence boundary.
- Removed the editable Master wrapper from transient SVG, Designbot and live-experiment proof rendering.
- Read interpolation compatibility diagnostics from canonical source layers instead of Master projections.
- Moved application grid-cache rebuilds and constant-time glyph lookup onto canonical paint-ready entries.
- Read application source paths, counts, writability and export participation from canonical source views and metadata.
- Read editor joining checks, SVG export, source-layer commands, Unicode parsing, kerning/groups, related glyphs and overview points from canonical caches and views.
- Moved application undo/redo enablement, mixed metadata ordering and asynchronous foreground revision checks onto Project-owned canonical layer history and revisions.
- Removed the application's active-Master and mutable-Norad accessors; format-boundary assertions materialize detached source snapshots.
- Python Babelfont package imports now construct canonical single- and multi-source documents through the checked source-format boundary.
- Built interpolated sources as atomic canonical document transactions, preserving exact metadata and undo while assigning fresh object identities to the new source.
- Moved Designspace structure, font information, component alignment, mark color, metrics keys and formulas, metaball payloads, and HOI intermediate points into typed canonical document storage with guarded edits and UFO boundary projection.
- Added failure-atomic Unicode replacement across every canonical source layer with one document revision and exact save/reopen persistence.
- Made semantic glyph-mark edits update or clear the Runebender label and public UFO color atomically while preserving exact source color spelling on a no-op.
- Moved quadratic, cubic, hyperbezier, corner-rounding, handle-harmonizing, handle-balancing, and handle-optimization operations onto canonical contours.
- Moved hyperbezier pen creation, point appending, and closure onto typed canonical contours with stable object identities.
- Added atomic canonical contour import for SVG append and image-trace replacement without whole-glyph reconciliation.
- Agent `SetOutline` operations now decode their public drawing payload directly into canonical contours and clear components only when explicitly requested.
- Removed the transitional detached-UFO whole-glyph edit bridge after moving every production caller to direct canonical layer operations.
- Moved background send, swap and clear into canonical auxiliary-layer transactions with guarded source-history undo and redo.
- Moved ordinary pen creation, segment appending, and closure onto typed canonical contours with stable object identities.
- Moved explicit mask baking onto canonical contours and cleared the persisted mask key only after successful subtraction.
- Moved selected and whole-layer metaball collapse onto staged canonical layer edits that retain live groups until conversion succeeds.
- Made image placement install validated source resources through Project without mutable UFO-font access.
- New fonts and in-memory UFO imports now enter through common canonical Project constructors.
- Native New Font now saves and opens that canonical Project directly without an application-side UFO round trip.
- Native conflict detection now watches nested external OpenType feature includes as well as UFO and Designspace roots.
  The browser-compatible UFO boundary preserves its supported metadata and default-layer glyphs while rejecting extra layers, images, data and unsafe or inconsistent paths instead of silently dropping them.
- Replaced long-lived full UFO persistence templates with explicit glyph-free source-format records while preserving layer order and paths, residual metadata, images, data and opaque payloads.
- Composition now writes complete revision-checked proposal plans atomically and keeps the foreground unchanged until explicit installation.
- Moved live proposals and experimental versions onto stable source-identified canonical layers with revision-checked installation and Project-owned undo.
- Removed Master-local proposal install/discard and undo wrappers after the editor and live tools moved to canonical Project proposal transactions.
- Moved source groups and exact fractional kerning into stable source-identified canonical metadata with atomic edits and UFO boundary rehydration.
- Made Project own variable glyphs and their layers, with guarded UFO projections for existing editing tools and shared history.
- Unified mapped-axis conversion and glyph-local interpolation behind Runebender-owned APIs using pinned Babelfont and fontdrasil adapters.
  Interpolation preserves fractional advances and kerning and varies anchors and component transforms.
- Switched interpolation inputs and compiled group/kerning snapshots to canonical document values, preserving layer paint order and checking numeric quantization.
- Reinterpolation now commits and verifies canonical layer transactions without exposing an intermediate whole UFO glyph.
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

- The bottom preview now follows live metaball dragging, including occurrences in proof text.
  Metaball parameters use sliders with numeric readouts; each drag is one undo step.
  X/Y sliders move selected centers together, preserving their spacing.

- Read cached master compatibility counts during view rebuilds, avoiding repeated whole-font interpolation on the UI thread.

- Metaball conversion now fits across inflections to reduce unnecessary nodes, while preserving extrema and the fitting tolerance.
  Converted contours start at their bottommost node.

- Metaball conversion now uses img2bez to fit between field extrema and inflections, with exact horizontal and vertical handles at extrema.
  Circle nodes stay on the four extrema, while blended outlines retain their structural points and fractional coordinates.
  Live metaballs remain editable until explicit conversion; conversion still participates in normal undo/redo.

- Restored stable component selection when undoing and redoing component additions, duplication and deletion.
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
