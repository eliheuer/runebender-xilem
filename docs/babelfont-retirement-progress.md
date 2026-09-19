# Babelfont compatibility retirement map

This M13 audit is anchored to integration commit `1f99ed0225cccd1933e6787c1bbb89590b027e7d`.
Line numbers below describe that commit and are not intended to float with later application work.
The audit covers production callers of `Master`, legacy glyph history, mutable source guards, long-lived UFO templates and the temporary canonical transaction bridge.
Tests and source-format codecs are listed separately from live editing callers.

## Acceptance boundary

Final M13 acceptance requires deleting `CanonicalLayerTransaction::compatibility_glyph`, `CanonicalLayerTransaction::reconcile_compatibility_glyph` and their `LayerEditDraft` implementations.
The bridge is a migration tool, not an allowed architecture boundary.
Norad may remain inside explicit UFO, Designspace and imported-format codecs and in fixtures that exercise those codecs.
Application state, `Project`, canonical history and editing commands must not retain or mutate a live Norad document.

## Temporary transaction bridge

The recorded tree has no production caller of the bridge.
Only the public wrappers at `src/document/project.rs:162-175` and the private implementations at `src/document/babelfont.rs:736-740` remain.
The active M06 worktree is expected to use the bridge transiently while deleting `Session`'s stored UFO glyph.
Every such use must be replaced by a direct `LayerEditDraft` operation before M13 deletes the four methods.

Current integration update: all four methods are deleted after the application background callers moved to canonical Project transactions.
Application fixtures obtain read-only detached glyphs through `formats::ufo`; that explicit codec has no reconciliation operation.

The separate `edit_batch::SetOutline` path no longer projects a draft to a UFO glyph or calls `reconcile_layer_from_ufo`.
It decodes the public drawing payload once, replaces canonical contours directly and removes stable components only when the operation requests it.
The unused `Project::reinterpolated_from_others` whole-glyph return path is also deleted; reinterpolation commits through `reinterpolate_document_layer`.

## Legacy glyph history callers

The canonical replacement already exists in `Project::begin_document_layer_transaction`, `commit_document_layer_transaction`, `document_layer_history_depth`, `can_replay_document_layer_history` and `replay_document_layer_history`.
These APIs own exact layer snapshots and stable `GlyphLayerAddress` identities.

| Production caller | Legacy dependency at the recorded commit | Minimal retirement step |
|---|---|---|
| `application/actions.rs:123,132` `enabled` | `Master::can_undo` and `can_redo` | Query the active canonical layer address and `can_replay_document_layer_history`. |
| `application/editor/commands.rs:49,104,1252` component commands and paste | `Master::undo_depth` is captured for mixed UI history ordering. | Capture `document_layer_history_depth` for the exact address. |
| `application/editor/inspector.rs:83,331,391,474,621` metadata ordering | `Master::undo_depth` is compared with application metadata snapshots. | Store and compare canonical addressed-layer depth. |
| `application/editor/session.rs:2070-2116` `sync_session_from` | Session drains `HistoryOp` records into `Master.history`. | Commit only `CanonicalLayerTransaction` values and remove `HistoryOp`. |
| `application/editor/session.rs:2165-2170` `undo_open_glyph` | `Master::undo` and `redo`. | Replay the active `GlyphLayerAddress` through Project history. |
| `application/editor/session.rs:2228-2243` overview batch undo | `SourceEdit` plus per-master undo and redo. | Replay each recorded canonical layer address in the existing application batch order. |
| `application/editor/session.rs:2410-2424` `refresh_open_glyph` | Inspector snapshots are drained into `Master.history`, then a whole glyph is replaced. | Commit the inspector's pending canonical transaction and reload its layer view. |
| `application/editor/inspector.rs:677-690` overview advance | `record_undo`, `set_advance` and `discard_last_undo`. | Use one canonical layer transaction and let an unchanged commit create no history. |
| `application/editor/inspector.rs:713-714` overview marks | `record_undo` plus `Master::edit_glyph`. | Use `LayerEditDraft::set_mark` in one transaction per selected layer so the label and public color remain atomic, retaining the existing overview batch ordering. |
| `application/editor/tools/metaballs.rs:314-315` font-wide collapse | `record_undo` plus whole-glyph replacement. | Use one addressed canonical transaction per changed glyph and replay them through the existing `OverviewEditBatch`. |
| `application/editor/tools/local_ai.rs:409-465` proposal install, undo and discard | `Master::install_proposal`, `Master::undo` and mutable UFO proposal layers. | Use existing `proposal::install_project`, `proposal::discard_project` and Project history replay. |
| `application/editor/commands.rs:265-274` overview reinterpolation | Direct `Master.history` record and glyph mutation. | Call the existing `Project::reinterpolate_document_layer` transaction path. |
| `application/editor/commands.rs:496-509` mask baking | Direct per-master history and UFO mutation. | Add or use a direct `LayerEditDraft` mask operation, then commit addressed transactions. |
| `document/project/glyph_transactions.rs:245-258` glyph remove and rename | Clears or renames both canonical and per-master history. | Keep only `DocumentHistory::clear_glyph` and `rename_glyph` after Master history is removed. |

`Project::edit_layer` and `Project::undo_layer` have no production caller at the recorded commit.
Their remaining uses are compatibility tests in `tests/variable_project.rs`, including `source_undo_refuses_to_overwrite_later_edits_and_layer_operations_preserve_other_glyphs` and `layer_edits_and_history_round_trip_all_source_data`.
Those tests must be rewritten against owned canonical transactions and Project history, after which both methods can be deleted.

## Mutable source guards

`SourceEdit`, `SourceFontEdit` and `SourcesEdit` are defined at `src/document/variable.rs:948-1026` and exported through `Project` at `src/document/project.rs:1421-1459`.
Their drop implementations reconcile a mutated UFO back into canonical ownership, so every production call is a live compatibility dependency.

| Production caller | Current mutation | Minimal prerequisite or existing replacement |
|---|---|---|
| `application/editor/commands.rs:291` `command_update_metrics` | Iterates every mutable `Master`, reads metrics keys and edits outlines and advances. | Existing canonical metrics-key reads, bounds and layer draft translation/width APIs are sufficient. |
| `application/editor/commands.rs:496` `command_bake_masks` | Runs the UFO mask helper in every source. | A direct `LayerEditDraft` mask operation is the only missing engine operation. |
| `application/editor/commands.rs:673` `save_as_to` | Rewrites source paths and dirtiness through `SourcesEdit`. | The M12 Project-owned retarget transaction must also relocate relative feature includes before publication. |
| `application/editor/commands.rs:815` `command_place_image` | Inserts source image bytes through `FontModel::font_mut`. | A guarded canonical source-resource transaction is required for image bytes; `LayerEditDraft::set_image` already covers the glyph reference. |
| `application/editor/session.rs:2228` overview history | Uses `edit_source` only to reach legacy history. | Remove with the history caller described above. |
| `application/font_model.rs:494,517` background layer writes | Creates, writes and clears a UFO background layer. | Use `Project::copy_document_layer_to_background`, `swap_document_layer_with_background` and `clear_document_background`; these canonical source-history transactions now cover standalone UFO and Designspace documents. |
| `application/font_model.rs:708` overview advance | Calls `Master::set_advance`. | Existing `LayerEditDraft::set_width` is sufficient. |
| `application/font_model.rs:739` Unicode propagation | Mutates every source UFO glyph. | Existing canonical source-glyph metadata transaction is sufficient. |
| `application/font_model.rs:764` `replace_glyph` | Replaces a whole active-source UFO glyph. | Delete after Session, metaball and other whole-glyph callers move to direct layer drafts. |
| `application/editor/tools/metaballs.rs:314-315` | Records legacy history and replaces UFO glyphs. | Existing metaball decoding plus canonical layer draft conversion is sufficient. |
| `application/editor/tools/local_ai.rs:400,412,448,456` | Writes, installs, undoes and discards UFO proposal layers. | Existing Project proposal APIs are sufficient. |

The `font_mut` occurrences in `application/editor/tools/text.rs` and `application/editor/tools/nodes.rs` are test-only fixtures after their containing `#[cfg(test)]` boundaries.
The `edit_sources` occurrences in `application/platform/host.rs`, `document/live.rs`, `formats/babelfont_import.rs`, `formats/designbot.rs` and `document/filesystem.rs` are likewise fixture-only at the recorded commit.

Current integration update: local-AI proposal adoption, preview, list, installation, discard, Cmd+Z and dedicated Undo Install use canonical Project APIs and addressed layer history.
`Master::install_proposal` and `Master::discard_proposal` are deleted; the standalone UFO helpers remain only for the explicit external contract and fixtures.
Canonical `Project::document_glyph_codepoints` and `set_document_glyph_codepoints` now supply the Unicode read and atomic all-source write boundary; the application caller is the remaining cutover step before its mutable-source dependency can be removed.

## Long-lived source templates and projections

`VariableData.templates` is a `BTreeMap<SourceId, norad::Font>` at `src/document/variable.rs:301`.
It is cloned into structural snapshots, filled by constructors, updated by source creation and glyph transactions, read by proposal staging, and cloned by `source_font` for serialization.
Although glyph payloads are cleared, this is still long-lived Norad document state and therefore is not an M13 codec allowlist item.

The minimal replacement is a typed `SourceFormatData` record owned by the document.
It must preserve layer order, layer names and paths, layer lib and color, UFO meta, opaque font and layer lib fields, data and image resources, and the exact GLIF path map without storing a `norad::Font`.
`filesystem::ImportedUfo` should decode this record at import, while `SourceExport` should combine it with canonical metadata and layers only at staged serialization.

The exact template-dependent production sites are:

- `document/variable/constructors.rs:79` inserts decoded templates.
- `document/variable/source_builder.rs:133-193` validates, clones and inserts a template for a new source.
- `document/variable/glyph_transactions.rs:262` updates template structure during whole-glyph transactions.
- `document/variable/proposal_transactions.rs:22,62` uses templates to validate and publish proposal layers.
- `document/variable.rs:316,372,405,655,714,849-885` clones, restores, edits and serializes templates.

Implementation update: `Replace UFO templates with source format data` removes this full-font map and replaces it with `SourceFormatData`.
The record owns glyph-free layer order, names, exact paths, layer libs and colors, UFO metainfo, residual font info and lib values, plus data and image stores.
Canonical features, groups, kerning, font information, glyph metadata and geometry remain outside it.
Transient `norad::Font` values are reconstructed only when an existing compatibility or persistence boundary explicitly requests a source snapshot.
The 74 variable-project tests and eight staged-filesystem tests preserve the prior exact save behavior; deleting the remaining source-snapshot consumers is still M13 work.

`Master` remains a second live source model at `src/document/source.rs:84` and `Project.masters` at `src/document/project.rs:290`.
Its `font`, paint cache, path, dirty flags, preserved files and legacy history mix application caches, persistence state and editing ownership.
Before deleting it, split the noncanonical responsibilities into a source shell containing only source path, preserved filesystem payload and save status, and derive application glyph caches from canonical views.

## Read-only Master callers

The main production read surfaces are `FontModel::master` and `FontModel::font` at `application/font_model.rs:176-192`.
They feed cache rebuilding, glyph lookup, background and proposal previews, export filters, node and local-AI revision checks, joining checks, SVG export and source-panel layer names.
Canonical replacements already exist: `document_source`, `document_sources`, `document_glyph`, `document_layer`, canonical glyph revisions, source metadata views and the typed renderer.
Project compatibility detail no longer joins this list: it compares canonical layer topology directly, and the unused `feature_source` Master accessor is deleted.

The remaining non-application consumers are:

- `formats/babelfont_import.rs:138-300` assembles Python multi-source imports through `Master` and `Project::from_designspace`.
- `document/filesystem.rs:16,124-143` converts a validated `ImportedUfo` into `Master` during load.
- `document/project/constructors.rs:57` still creates a compatibility `Master` after canonical in-memory construction.

The CLI `info`, `proof`, proposal list/install/discard, agent source selection and `project_info` callers now consume Project source views and canonical proposal operations; proposal mutations save through `Project::save`.
The remaining detached SVG entry point, Designbot and live experiment proof accept transient source-font boundary values without constructing a Master; moving the live snapshot itself to typed Project rendering remains.
Python Babelfont and filesystem imports need a canonical multi-source constructor that accepts decoded source records and canonical Designspace data without a `Master` callback.

## Source-history cleanup in the owned lane

`SourceHistory` still parks `Master.history` and `VariableData.histories` in `src/document/sources.rs:139-186`, moves them while rebuilding projections at `src/document/sources.rs:212-274`, and parks them during removal at `src/document/sources.rs:579-587`.
Project-owned `DocumentHistory` is already keyed by stable `GlyphLayerAddress`, so its piles can remain parked naturally while a source identity is absent and become addressable again when structural undo restores that source.
Once application callers stop writing legacy history and the shared fields are removed, this owned lane can delete both retired maps, the park/reconcile helpers and every history transfer during source projection rebuild.
The source structural tests must add an explicit canonical-history-before-remove, undo-remove, replay-layer-history regression before that cleanup lands.

## Removal order

1. Finish M06 Session and application caller cutover, using direct canonical operations wherever they already exist.
2. Add only the two identified missing editor prerequisites: direct mask baking and guarded source image-resource mutation.
3. Move local-AI and Nodes proposal reads, installs and discards to existing Project proposal APIs.
4. Move the remaining SVG compatibility entry point, Designbot and live proof to Project source views and typed rendering; the CLI proof/info/proposal callers are complete.
5. Land the M12 Project retarget transaction with feature-include relocation and the canonical Python multi-source constructor.
6. Replace `VariableData.templates` with typed source-format preservation data and make filesystem serialization the only Norad reconstruction point.
7. Replace `Project.masters` with the source shell, derive application caches from canonical views and delete all mutable source guards.
8. Delete legacy `Master.history`, `VariableData.histories`, `Project::edit_layer`, `Project::undo_layer` and the source-history parking code after the source-removal history regression passes.
9. Complete: delete the compatibility glyph bridge and its reconciliation wrapper.
10. Run a final search-based architecture gate that permits Norad only in named codec modules and fixtures, followed by the full M14 native, browser, preservation and clean-checkout proof.

## Audit commands

The map was produced with production searches over `src/**/*.rs`, with test-module occurrences classified separately, plus focused inspection of `project.rs`, `variable.rs`, `source.rs`, `sources.rs`, application callers and format adapters.
The final M13 gate must repeat those searches against the final integration commit rather than relying on this recorded map.
