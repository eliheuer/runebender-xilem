# Changelog

All notable changes to runebender-xilem. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

- Align Kerning pair rows and let its three editor fields share the resized dock width.

- Align the Dimensions readout row heights and column spacing.

- Show the active master and align existing overview glyph identity fields.

- Align sidebar counts, selected-row borders and compact filter rows.

- Restore the Separator category and match sidebar section spacing.

- Give glyph search equal-width scope, regex and case toggles and align its editor row.

- Fit complete overview tile rows above a compact footer and keep grid margins clear.

- Make both side panels and the proof strip resizable by dragging their dividers.
  Dock widths remain stable when the window resizes, with usable minimum sizes.

- Match editor sidebar tab spacing and faces, joining the selected tab to its panel.

- Retain dark keylines on enabled toggles and use crisp field borders in the metrics card.

- Match Background control rows and inspector field alignment; the Background toggle
  now reflects the visibility setting even when the current glyph has no background.

- Align inspector group dividers, coordinate rows and curve controls with the reference.

- Match the Coordinates inspector's boxed reference picker and balanced numeric fields.

- Give glyph-grid tiles flat faces, crisp inside borders and a matching panel ground.

- Show contour direction with a separate start arrow while preserving each
  point's corner or smooth color; open contours do not get start arrows.

- Distinguish anchors with filled diamonds and pink keylines, scaled with zoom
  alongside outline points and retaining a clear selected state.

- Scale outline point markers smoothly with zoom, keep selected points legible
  with a dark keyline, and draw handle lines in the shared neutral ink.

- Keep metric rules within the glyph advance and extend its frame to the full
  em height, matching the reference canvas without lines across the workspace.

- Restore the Gray reference palette's lighter panel and canvas surfaces and
  quieter field borders using the shared OKLCH tokens.

- Align transformation icon rows with their heading and give inspector actions
  and parameters a consistent 32-pixel row pitch.

- Draw inspector transformation icons at the measured 22-pixel reference size
  for clearer shapes and matching visual weight.

- Complete the inspector's transformation row with both rotation directions,
  Duplicate, and Duplicate Repeat. The clockwise icon now rotates clockwise.

- Add working Stroke width and Fit curve % inspector fields. Enter applies the
  core operation to the selection (or all contours/curves), with undo; invalid
  values and unchanged fits leave the document untouched.

- Keep path operations visible with Transformations, with single-line parameter
  fields and Add extremes. Put Glyph first in the inspector, show selection
  status in Coordinates, and open Background by default.

- Label the floating spacing card's LSB and RSB fields, keep kerning groups on
  their own row, and match the compact neutral card's canvas placement.

- Fit five compact glyph columns in the editor rail, with whole-pixel thumbnails
  and an independent size slider beside the filtered glyph count.

- Give the proof drawing its full initial height: move Invert/Blur into the
  compact editor footer and preview text into Shaping, alongside direction,
  features, and language. Match proof fitting and the single header divider.

- Keep the editor proof strip, including its controls, within its 140-pixel
  initial height so the canvas has more room and the initial glyph fit is larger.

- Make Unicode and glyph-name changes atomic across designspace masters and
  Undo/Redo them in order with surrounding outline and metric edits; preserve
  each renamed glyph's existing Core history and add overview-width Undo/Redo.

- Preserve Core Undo/Redo for inspector Unicode, width, and sidebearing edits;
  reject non-finite metrics, keep advance fixed when changing LSB, and report
  rejected glyph-name collisions without leaving a stale field value.

- Connect the Chat rail to local GGUF models through `font-ml chat` and the
  editor's private live endpoint, with streamed transcript/tool rows, model
  choice, multi-turn context, cancellation, clearing, and proposal refresh.

- Require an explicit user-authorization argument before live automation can
  install proposals or apply and undo experiments; keep socket operations on
  the editor-owned unsaved document without writing its UFO source.

- Paint disclosure and leaf markers as theme-aware vector geometry so the
  bundled interface font cannot turn sidebar and inspector state into missing-glyph boxes.

- Add reproducible 1x and 2x headless parity captures with fixed logical sizing,
  device scale, fixture hashes, and renderer metadata.

- Make overview mark-color changes update the visible grid immediately and
  undo a multi-glyph selection as one source- and glyph-identified edit.

- Include glyph metadata and lib data in Core undo snapshots, so mark colors,
  Unicode, notes, images, and guidelines restore with outlines and metrics.

- Route text-tool input through native IME composition and commit events,
  preview preedit text without changing the buffer, and prevent key/IME duplicates.

- Add bidi-aware keyboard text selection, replacement, and deletion, with visible
  selection and logical pointer mapping across Arabic ligatures.

- Reshape existing text immediately after live glyph, metric, or feature refresh
  while preserving its caret, selection, active glyph, and manual kerning state.

- Add shared text-tool and preview controls for common OpenType features and
  automatic, Arabic, or Urdu shaping locale selection.

- Support system clipboard copy, cut, paste, and select-all in the canvas text
  tool using logical Unicode text, normalized line breaks, and Arabic reshaping.

- Keep editor text, preview text, direction, language, and feature choices with
  each editor tab, without carrying a widget-owned text buffer into another tab
  or replacement document.

- Match GPUI's overview and editor-rail grid sizing, selection extension, caption
  geometry, and source-derived padding; render square cells as true rectangles so
  CPU evidence retains Gray-theme outlines and labels.

- Keep document navigation clean, refuse external reload while unsaved edits are
  present, and exercise outline, metrics, anchor, undo/redo, save, and reopen on a
  disposable full Virtua Grotesk designspace without changing unrelated font data.

- Keep every local-AI result as a reviewable proposal, including single-glyph
  runs; preserve complete worker diagnostics and require an explicit Install or
  Discard before the foreground changes.

- Reject completed local-AI and node results after document, master, glyph, or
  foreground-revision changes, including all-glyph runs whose inventory changed.

- Surface `font-ml tasks --json` launch, exit, schema, and JSON errors in the
  Local AI rail while keeping panel task availability aligned with node types.

- Wire the Nodes toolbar Open action to the existing native graph picker and
  cover new/save/reopen, parameter, validation, and failure-state round trips.

- Route multiline text-tool Up/Down/Home/End keys through Core's bidi-aware caret
  model and verify mixed Latin, Arabic, digits, lam-alef, and kasra against the
  real Virtua Grotesk inventory and feature file.

- Make node runs target the open or explicitly selected glyphs, resolve sibling
  designspace masters, preserve model-device choice, and return proposal-only
  graph output to the same Compare, Install, Discard, and Undo review workflow.

- Align the navigation, grid and inspector top edges with one shared separator below the title bar.

- Align search and inspector inputs with shared insets and optical baseline positioning; route inspector text and labels through the bundled UI-font helpers.

- Preserve the native macOS editor menus at startup by disabling winit's replacement default menu.

- Give glyph search equal padding and matching Virtua Grotesk typography for placeholder and typed text, preventing clipped descenders.

- Remove the doubled right edge on the navigation tabs and keep scrollbar overlays outside the viewport clip during scrolling, preserving Masonry scrolling behavior.

- Match the mark strip to GPUI with equal slots, smaller circles, selection rings, and a centered drawn clear cross; restore vertical panel dividers.
- Use the native application properties for headless visual captures.

- Apply the bundled Virtua Grotesk at 13px consistently to UI labels and editable fields.

- Complete the desktop menu system: shared command metadata, native macOS and
  accessible in-window menus, working file/glyph/path/filter/view commands,
  stateful submenus, keyboard navigation, and focused-text shortcut precedence.

- Consolidate Core and its headless CLI into the Xilem Cargo workspace, preserving Core history and the existing theme.

- Make the main checkout the primary Xilem development location; retain GPUI as a reference and fallback.

- Keep wheel and trackpad scrolling without visible scrollbar overlays; verified during pointer movement, two-axis scrolling, and resizing.

- Correct the header separator and outline the navigation strip; restore the Chat tab with an explicit unavailable state.

- Match navigation tab heights, top-only selected corners, and icon sizing to GPUI.

- Remove the extra titlebar icon and tighten category sidebar rows to the GPUI reference.

- Count the primary overview selection and contain glyph captions within their tiles.

- Retain the live text buffer across tool switches without consuming outline-tool input.

- Match GPUI continuity rings while preserving the underlying point markers.

- Draw curvature combs with GPUI-normalized geometry and theme-colored, outlined teeth.

- Add Fit graph and refit newly opened node files; keep glyph-grid controls in Font view.

- Fit the initial node viewport and keep connection wires and ports visible above cards.

- Use shared neutral theme colors for sliders and header tab selection.

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
