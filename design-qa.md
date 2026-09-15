# Design QA — overview glyph inspector reflow

**Source visual truth**

- User issue capture: supplied 2026-09-15 overview-inspector screenshot.
- GPUI behavior reference: `runebender-gpui/src/view/render.rs` and
  `runebender-gpui/src/view/panels/glyph_info.rs`

**Implementation evidence**

- Before, Gray: `docs/visual-audit/2026-09-15/221-xilem-overview-glyph-panel-before-gray-2x.png`
- Final, Gray: `docs/visual-audit/2026-09-15/222-xilem-overview-glyph-panel-final-gray-2x.png`
- Final, Light: `docs/visual-audit/2026-09-15/223-xilem-overview-glyph-panel-final-light-2x.png`
- Final collapsed state, Gray:
  `docs/visual-audit/2026-09-15/229-xilem-overview-glyph-panel-collapsed-gray-2x.png`
- Focused before/after: `docs/visual-audit/2026-09-15/227-xilem-overview-glyph-panel-before-after-gray.png`
- Normalized source/final comparison:
  `docs/visual-audit/2026-09-15/228-source-final-overview-glyph-panel-gray.png`

**Normalization and state**

- Source capture is 580 × 1670 physical pixels and shows the Gray overview
  inspector with `A` selected and Glyph expanded.
- Implementation captures are 2400 × 1600 physical pixels for a 1200 × 800
  logical viewport at 2× density, in the same selection and disclosure state.
- The source and final right rails were normalized to 492 × 1600 for the focused
  comparison. The source came from a taller native window, so component geometry
  and flow—not absolute window height—are the comparison target.

**Findings**

- No remaining P0, P1, or P2 differences in the requested inspector scope.
- Fonts and typography retain the shared interface family, sizes, and line
  boxes. Glyph field labels now use the same structural ink as section keylines.
- Spacing and layout now keep every disclosure row in normal flow. Glyph name
  owns a full row; Width and Unicode share the next row; the preview receives
  all remaining height and recomputes when a section opens or closes.
- Colors and tokens use the semantic panel, field, and outline roles in both
  Gray and Light. Input borders now match the section keyline instead of using
  the quieter field-outline token.
- Image and icon fidelity are unchanged: the preview is native outline geometry,
  and this change introduces no raster assets or replacement icons.
- Copy/content removes the redundant Master/Regular fact while preserving the
  editable glyph name, width, and Unicode values.

**Comparison history**

- Earlier P1: the fixed splitter retained its collapsed-section position after
  Glyph expanded, clipping Kerning through Masters behind the preview. Replaced
  it with a natural-height section stack plus a flexible preview. Post-fix
  evidence `222` and `227` shows every section above the preview.
- Earlier P2: three full-width field rows and the Master fact consumed excess
  space, while field borders and labels were visually weaker than section
  structure. Removed the fact, paired the short fields, and applied the outline
  role to labels and borders. Post-fix evidence `228` shows the compact hierarchy.

**Implementation checklist**

- [x] Keep expanded sections above the preview without overlap.
- [x] Recompute preview height from the remaining inspector space.
- [x] Remove the redundant master row.
- [x] Give Glyph name a full row and pair Width with Unicode.
- [x] Match field labels and borders to the structural outline.
- [x] Verify matched Gray and Light captures at 2× density.
- [x] Verify the preview expands again when Glyph is collapsed.

final result: passed
