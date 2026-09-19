# Babelfont migration baseline inventory

Reviewed at `624879a1c447d3e9f012c34f4b5cb091bb0df6cb` on 2026-09-18.
This is the M00 baseline, not the M13 codec allowlist.
The milestone assignments cover runtime behavior even when a module has no literal Norad import.
No existing editing module is exempted by its directory or by a type alias.

## Production families

Paths in the following table are relative to `src/`.
Each path mentioning Norad in production is assigned below; a family can require several dependent milestones.
The “boundary” classification means serialization is appropriate there, not that all its current APIs may remain unchanged.

| Paths | Current use and required migration | Milestones |
|---|---|---|
| `document/variable.rs`, `document/babelfont.rs` | Persistent Norad glyphs and glyph-free font templates coexist with Babelfont geometry; index-based restoration and guard reconciliation must become exact extensions and identity-aware boundary conversion. | M01, M02, M12, M13 |
| `document/project.rs`, `document/source.rs` | Full source fonts, mutable Master operations, derived paint caches, loading/saving, interpolation and public Norad accessors; split codec operations from canonical document editing. | M02, M03, M05, M08, M11, M12, M13 |
| `document/sources.rs` | Structural snapshots clone both models and Designspace; source authoring and undo must use canonical data and stable identities. | M05, M08 |
| `document/history.rs` | Norad glyph snapshots keyed by glyph name; migrate all retained editable fields and grouping semantics. | M05 |
| `outline/glyph_ops.rs`, `outline/point_ops.rs`, `outline/segment_ops.rs`, `outline/glyph_paths.rs` | Ordinary geometry, conversion, mutation and GlyphSnapshot APIs expose Norad. | M03, M04, M05 |
| `outline/cleanup.rs`, `outline/component_ops.rs`, `outline/convert.rs`, `outline/drawing.rs`, `outline/effects.rs`, `outline/embolden.rs`, `outline/knife.rs`, `outline/metaballs.rs`, `outline/path/hyper_model.rs` | Special tools and legacy path bridges still produce/edit Norad; preserve editable hyperbezier/metaball data and move topology operations onto document geometry. | M04 |
| `analysis/curve.rs`, `analysis/dimensions.rs`, `analysis/glyph.rs` | Read/query APIs and proof analysis consume Norad glyphs/fonts. | M03, M11 |
| `application/editor/session.rs`, `application/font_model.rs`, `application/workspace.rs` | Session glyphs, pending records, clipboard, font access and metadata snapshots retain Norad; switch to document transactions and presentation queries. | M05, M06, M07 |
| `application/editor/commands.rs`, `application/editor/inspector.rs` | Tool, image, font metadata and history commands operate through legacy accessors. | M06, M07 |
| `application/view/canvas/editor.rs`, `application/view/panels/editor_info.rs`, `application/view/panels/preview.rs` | Canvas point types, font-info readers and panels expose source types. | M06 |
| `document/font_ops.rs`, `document/composites.rs`, `document/compose.rs`, `document/model/glyph_metadata.rs`, `ui/theme.rs` | Font/glyph edits, alignment, category/export metadata and mark-color readers/writers use Norad. | M07 |
| `document/interpolation.rs` | Input/output geometry is Norad despite the shared numeric backend. | M08 |
| `document/compile.rs`, `document/compile_metadata.rs` | Compiler metadata and participation still read Norad projections; construct immutable compiler snapshots from canonical ownership. | M09 |
| `text/buffer/mod.rs`, `text/features.rs` | Character/kerning inventories and feature generation accept Norad; preserve compiled-font shaping and font-wide feature semantics. | M07, M09 |
| `document/proposal.rs`, `document/edit_batch.rs`, `document/experiments.rs`, `document/live.rs`, `document/nodes_live.rs`, `application/editor/tools/local_ai.rs` | Proposals, revision hashes, live edits, full experimental font copies and selective apply depend on Norad/Master. | M10 |
| `application/cli.rs`, `document/nodes_run.rs`, `formats/designbot.rs` | Headless source loading and editing, Nodes file operations and proof/kerning adapters consume legacy fonts. | M11 |
| `document/font_memory.rs`, `document/new_font.rs`, `application/browser.rs` | Mixed source decoding, construction and Master installation; converge on shared document constructors. | M12 |
| `formats/babelfont_import.rs`, `formats/binary_import.rs`, `formats/glyphs_import.rs`, `formats/designspace.rs` | Source-format boundary codecs; replace legacy construction outputs while retaining supported precision, unknown-data and rejection guarantees. | M01, M12 |
| `formats/svg.rs`, `formats/image_trace.rs` | External geometry decoding currently returns Norad outlines for insertion; boundary conversion and document insertion must stay distinct. | M04, M12 |
| `formats/lib_keys.rs`, `formats/metaballs.rs`, `formats/metrics_keys.rs`, `formats/color_font.rs` | Mixed persisted-key codecs and live metadata/geometry operations; migrate live behavior instead of exempting the files wholesale. | M04, M07, M12 |

The knife module is a useful search hazard: production Norad bridge functions appear after a test module.
Everything after the first `#[cfg(test)]` cannot simply be discarded from the inventory.
Likewise, test imports do not imply that a file's production callers are independent of the legacy model.

## Tests and comments

These classifications concern the literal Norad mentions, not permission to leave indirect runtime model access in place.

| Paths relative to `src/` | Literal mentions | Runtime follow-up |
|---|---|---|
| `application/editor/tools/metaballs.rs` | Test fixtures | Tool callers migrate with M04/M06. |
| `application/editor/tools/nodes.rs` | Test fixtures | Live and file Nodes callers migrate with M10/M11. |
| `application/editor/tools/text.rs` | Test-only helpers and fixtures | Compile/preview queries migrate with M09. |
| `application/platform/host.rs` | Test fixtures | Load/save/reload and source conflicts remain M12 obligations. |
| `application/view/render.rs` | Test fixtures | Shared view construction remains M06/M14 coverage. |
| `text/buffer/tests.rs` | Dedicated test module | Convert operation tests to the canonical API; retain explicit codec fixtures where needed in M09/M12. |
| `text/shape.rs` | Test fixture | Keep the binary shaping boundary and M09 coverage. |
| `ui/sidebar.rs` | Test fixture | Preserve sidebar source behavior with M06. |
| `lib.rs`, `outline/mod.rs` | Model descriptions | Update inaccurate ownership descriptions at M13 or earlier when the model changes. |
| `outline/path/cubic.rs`, `outline/path/point.rs` | Descriptions of the workspace contour bridge | Review legacy intermediate path conversions under M04; comments are not production Norad imports. |

Outside `src/`, literal mentions are in `tests/cli.rs`, `tests/mark_features.rs`, `tests/variable_compile.rs`, `tests/variable_project.rs`, `examples/live_design_fixture.rs` and `examples/metaball_fixture.rs`.
The tests may keep explicit codec fixtures but their assertions must exercise canonical editing operations after migration.
The examples generate disposable proof fixtures and remain M12/M14 integration obligations.
`tests/babelfont_contract.rs` has no literal Norad mention but is essential evidence for the exact-value contract.

## Indirect routes and compatibility APIs

The baseline search for `SourceEdit`, `SourceFontEdit`, `SourcesEdit`, `active_font_mut`, `edit_source(s)`, `open_master` and `font_mut` complements the literal import inventory.
Additional routes include `application/editor/sources.rs`, `application/platform/live.rs`, `application/platform/export.rs`, `document/agent.rs` and the document Nodes dispatch modules.
Their owners are M06/M08, M10, M09, M11 and M10/M11 respectively.
The already separated `document/axis.rs` and `document/var_model.rs` numeric adapters remain M08 obligations without requiring another numeric backend.
The `web/` workspace reuses these sources and must be checked when shared APIs change.

| API or retained state | Baseline classification | Removal or replacement owner |
|---|---|---|
| `SourceEdit`, `SourceFontEdit`, `SourcesEdit`, `editing_parts`, `active_font_mut`, `edit_source`, `edit_sources` | Active production compatibility mutations; not obsolete yet. | Callers in M02–M12; delete in M13. |
| `Master.font`, `FontModel::font`, `font_mut`, `master`, `master_mut`, `master_font`, `feature_font` | Active production editing/read surface and retained full source fonts. | M06–M12, then M13. |
| `Project::edit_layer` and `source_snapshot` | The former still edits a Norad clone; the latter is a serialization projection also consumed by transitional code. | Direct edits in M02; boundary-only snapshot usage by M12. |
| `VariableData.glyphs`, `templates`, `synchronize`, `update_source` | Active mirrors and reconciliation, not just persistence codecs. | M01/M02, final deletion/consolidation M13. |
| `Master::amend_undo`, `EditHistory::amend` | No production caller of `Master::amend_undo` found in `src/`; the internal amend behavior has a test. Candidate compatibility API, not a required Session drag mechanism. | Review/remove with M05/M13 after callers are migrated. |
| `Master::snapshot_contours`, `restore_contours` | In-tree uses found in tests; public compatibility methods still expose full snapshots. Their names understate the fields preserved. | M05/M13; retain equivalent complete-state testing. |

No candidate above was deleted in M00.
M13 must repeat the inventory against the final tree, inspect aliases and conversion wrappers, and justify individual codec boundaries with executed checks.
