# Babelfont editor/application migration lane

Status: **ACTIVE — application ownership and canonical transaction integration in progress**.

This lane owns M06 application work under `src/application/`, focused application tests and this progress record.
The integration lane retains the canonical Project implementation, shared module wiring and central migration documents.
Parallel authorization changes lane ownership only; the migration checklist's preservation and completion requirements remain unchanged.

## Baseline and scope

The clean isolated worktree was advanced from `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da` to integrated commit `0b06ad2` and placed on `codex/babelfont-editor-m06`.
This lane owns editor sessions, application commands, `FontModel`, workspace state, canvas and panel presentation, platform session synchronization and browser application bootstrap when directly required by M06.
M10 and M11 proposal, live-tool, Nodes and headless workflow migrations remain outside this lane except for unavoidable compiling adapters.

## Integrated shared APIs

M06 integrated Project-owned canonical history wrappers from the core and history lanes.
The surface records and coalesces completed `GlyphLayerAddress` transactions, discards a no-op group, queries undo/redo and replays guarded history while returning invalidation information.
Session will not own another undo pile.

M06 also integrated an owned guarded canonical layer transaction for the editor island.
It begins from a `GlyphLayerAddress`, exposes the existing `LayerEditDraft` read/write surface, retains the exact base state and commits only when the addressed live layer still matches that base.
This lets the canvas preview a pointer gesture locally and commit once without retaining a mutable Norad glyph or reconciling a complete source font after input.
The draft now includes stable-ID component and anchor add/remove operations plus image replacement.

## Remaining shared API blockers

The guarded draft now covers stable point, component and anchor selection; owned gesture lifetime; canonical contour clipboard values; edit commands; metadata; and complete whole-layer rendering.
Integration commit `1b5234e` closed the final rendering gap with full-render single-component geometry while decomposition deliberately retains its existing integer-rounded structural-contour behavior.
No shared API blocker remains for the Session/canvas/clipboard cutover.

## Canonical source-metadata presentation slice

Implementation commit: `Read application metadata from canonical sources`.
Resolve its exact ID with `git log --format=%H --grep='^Read application metadata from canonical sources$' -1`.
Affected paths: `src/application/font_model.rs`, `src/application/editor/inspector.rs`, `src/application/editor/session.rs`, `src/application/platform/host.rs`, `src/application/view/panels/editor_info.rs` and this log.

`FontModel` now exposes the default source's canonical feature text and the active source's `CanonicalFontMetadata` directly from `Project`.
Kerning-group labels, kerning rows and group shelves no longer read paint-time values from a mutable UFO projection.
The panel retains exact fractional kerning values until its existing display formatting, and feature buffers now compare and reset against canonical text.
The existing generated-feature adapter still clones the compatibility source because M07/M09 have not yet supplied a canonical anchor inventory for that operation.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::font_model::tests:: -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --locked --bin runebender -- -D warnings
cargo fmt --all --check
git diff --check
```

All six focused `FontModel` tests passed, including a regression that commits unsaved canonical group membership and `-80.25` kerning and reads both through the application query surface.
Warning-denied binary Clippy, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

## Canonical font-info presentation slice

Implementation commit: `Read application font info canonically`.
Resolve its exact ID with `git log --format=%H --grep='^Read application font info canonically$' -1`.
Affected paths: `src/application/font_model.rs`, `src/application/view/panels/editor_info.rs` and this log.

`FontModel` now exposes canonical source-indexed font information and resolves the active UPM, ascender and descender from it.
The overview metadata rows and cross-master metrics, kerning counts, default-layer glyph coverage and advance comparisons no longer read those values from compatibility UFO sources.
All production session creation and rebuild paths now resolve layout metrics from canonical active-source font information; the Norad-only constructor remains test-only and transitional until the complete layer-draft cutover.
The focused regression commits an unsaved canonical family name and 2048 UPM with exact kerning metadata, then observes all values through the application query surface.
It also verifies a newly created session receives the canonical 2048 UPM and resolved ascender.
A two-source regression also verifies canonical default-layer glyph and advance comparison.

Integrated core commits `62dfaa8`, `3f48dc1` and `d9fbe05` provide typed font-info ownership, Project queries and validation-before-mutation.
An unrelated M07 glyph-metadata accessor that arrived in the source commit's context was excluded because its backing storage is not in this lane.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::font_model::tests:: -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --locked --bin runebender -- -D warnings
cargo fmt --all --check
git diff --check
```

All six focused `FontModel` tests passed.
All fifteen focused session tests passed.
Warning-denied binary Clippy, formatting and diff checks passed.

## Canonical feature-generation caller slice

Implementation commit: `Generate application features from canonical layers`.
Resolve its exact ID with `git log --format=%H --grep='^Generate application features from canonical layers$' -1`.
Affected paths: `src/application/editor/inspector.rs`, `src/application/font_model.rs`, `src/application/cli.rs` and this log.

The inspector now combines its unsaved feature draft with mark and mkmk features generated from canonical active-source layers.
It no longer clones the compatibility UFO to collect anchors, and it retains the existing review-before-Apply workflow.
The headless `features` command now loads `Project`, requires exactly one source and calls `features::generate_project` for canonical anchor resolution.
Its existing generated-file, JSON and human-readable output paths remain unchanged.

The lane synchronized with integration checkpoint `c22fa17` before this cutover.
That merge exposed and removed one obsolete duplicate `CanonicalSourceStructureSnapshot` block from the lane's older core ancestry; the retained definition includes canonical Designspace state and matches the integration branch.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::inspector::size_tests::generated_features_are_undoable -- --exact --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --locked --bin runebender -- -D warnings
target/debug/runebender --json features tests/fixtures/incompatible/Regular.ufo
cargo fmt --all --check
git diff --check
```

The generated-feature apply, undo, redo and save/reopen regression passed.
The headless canonical command returned its unchanged successful empty-feature JSON contract for the checked-in UFO fixture.
Warning-denied binary Clippy, formatting and diff checks passed.

## Project-owned source-metadata history slice

Implementation commit: `Move application metadata history into Project`.
Resolve its exact ID with `git log --format=%H --grep='^Move application metadata history into Project$' -1`.
Affected paths: `src/application/workspace.rs`, `src/application/editor/inspector.rs`, `src/application/font_model.rs` and this log.

Application history now stores only ordering context and labels for feature, group and kerning edits.
The exact before/after source metadata lives in `Project::SourceMetadataHistory`, and undo or redo replays that guarded canonical transaction before the application moves its ordering entry.
Undo enablement also requires the matching Project-owned history direction, so a stale or source-set-conflicted transaction cannot be advertised as replayable.

Kerning groups and exact `f64` pairs now mutate `CanonicalFontMetadata` through `Project::edit_document_source_metadata`.
The application no longer rewrites complete UFO group, kerning and feature maps to implement these edits or their history.
Core integration commit `fc3022f` synchronizes canonical metadata edits to the compatibility source, marks it dirty and sets `kerning_dirty` only when canonical groups or kerning changed.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::inspector::size_tests:: -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --locked --bin runebender -- -D warnings
cargo fmt --all --check
git diff --check
```

All eight focused inspector tests passed.
The kerning regression covers canonical group edit, Project-owned undo/redo, source dirty state and save/reopen persistence.
All fifteen existing session tests also passed against the integrated transaction and history APIs, preserving the current editor contract while the remaining canonical hooks are completed.
Warning-denied binary Clippy, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

## Project-owned component history slice

Implementation commit: `Move application edits into canonical transactions`.
Resolve its exact ID with `git log --format=%H --grep='^Move application edits into canonical transactions$' -1`.
Affected paths: `src/application/workspace.rs`, `src/application/font_model.rs`, `src/application/editor/commands.rs`, `src/application/editor/inspector.rs`, `src/application/editor/session.rs` and this log.

The component-add and alignment-toggle commands now commit guarded `CanonicalLayerTransaction` values at the active stable `GlyphLayerAddress`.
The alignment command translates the session's temporary component index to `ComponentId` before mutation and uses canonical cross-layer anchor resolution when re-enabling alignment.
Application history stores only the glyph, layer address, label and legacy ordering depth; exact before and after layer values remain in Project-owned history.
Undo and redo replay that guarded history and then rebase the open transitional session from the canonical layer projection.
That rebase is an explicit migration adapter and does not make the Norad-backed `Session` authoritative.

Integration commit `5cfe3a9` assigns one stable preserved component identifier when alignment metadata first creates a UFO object library.
Without that invariant, a legacy movement roundtrip generated a different identifier and correctly caused guarded history replay to reject the visually identical layer as stale.
The host regression covers canonical add, canonical alignment toggle, a later legacy movement, ordered undo and redo across both history systems, and save/reopen persistence.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked application::platform::host::tests::component_add_move_undo_and_save_reopen -- --exact --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused host regression passed.
The complete binary suite passed 166 tests with four documented model or external-font tests ignored.
Warning-denied workspace/all-target Clippy, formatting and diff checks passed.
The unchanged live-tool `Operation not permitted` diagnostics are sandbox-only and did not affect the file-backed editor assertions.

## Canonical whole-glyph lifecycle slice

Implementation commit: `Move application edits into canonical transactions`.
Resolve its exact ID with `git log --format=%H --grep='^Move application edits into canonical transactions$' -1`.
Affected path: `src/application/font_model.rs` and this log.

`FontModel` now delegates add, batch add-missing, duplicate, remove and rename to atomic canonical whole-glyph Project transactions.
The application no longer loops over mutable source projections for those commands or reconstructs duplicate payloads field by field.
The Project boundary retains active-source command semantics, stable logical glyph identity, sparse and auxiliary layers, source metadata, compatibility projection refresh and legacy history rename or cleanup.
`FontModel` now only maps transaction outcomes to the existing application return contracts and rebuilds its active-source presentation cache after a committed change.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::font_model::tests:: -- --test-threads=1
```

All six focused `FontModel` tests passed, including cross-source duplicate and remove behavior.

## Canonical composition caller slice

Implementation commit: `Write application composition proposals canonically`.
Resolve its exact ID with `git log --format=%H --grep='^Write application composition proposals canonically$' -1`.
Affected paths: `src/application/editor/commands.rs`, `src/application/cli.rs` and this log.

The editor command now plans composition from the active canonical source and writes the complete proposal layer through `proposal::write_composition_project`.
It no longer borrows the mutable compatibility font or calls the Norad `compose::compose` writer.
Empty plans retain the existing no-proposal report, while invalid or stale plans fail without partial proposal state.

The headless `compose` command now loads `Project`, selects the single UFO source, derives the same report through `compose::plan_project` and uses the guarded canonical writer only for a nonempty `--write` plan.
Its JSON and human-readable report schemas are unchanged, and a written proposal is persisted through `Project::save`.
Foreground layers remain unchanged until the existing explicit proposal-install command.

Executed evidence:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources CARGO_BUILD_JOBS=1 cargo test --locked --test cli compose_derives_marks_and_the_result_shapes -- --exact --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The compose CLI proposal and generated-font integration regression passed.
The complete binary suite passed 166 tests with four documented model or external-font tests ignored.
Warning-denied workspace/all-target Clippy, formatting and diff checks passed.

## Canonical glyph-inspection caller slice

Implementation commit: `Read CLI glyph analysis from Project`.
Resolve its exact ID with `git log --format=%H --grep='^Read CLI glyph analysis from Project$' -1`.
Affected path: `src/application/cli.rs` and this log.

The agent CLI's `read_glyph` helper now loads `Project`, selects its stable source and calls `analysis::glyph::read_project_glyph`.
The JSON schema and `glif-sha256:` revision contract remain unchanged, while bounds now use complete canonical smart-component, metaball and nested-component rendering.

Executed evidence:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources CARGO_BUILD_JOBS=1 cargo test --locked --test cli exact_edits_round_trip_without_rewriting_foreground -- --exact --nocapture
```

The focused read, proposal revision guard and foreground-preservation regression passed.

## Canonical editor selection and clipboard slice

Implementation commit: `Move editor selection and clipboard to canonical IDs`.
Resolve its exact ID with `git log --format=%H --grep='^Move editor selection and clipboard to canonical IDs$' -1`.
Affected paths: `src/application/editor/session.rs`, `src/application/editor/commands.rs`, `src/application/workspace.rs`, `src/application/view/canvas/editor.rs`, `src/application/view/panels/tabs.rs`, `src/application/view/panels/editor_info.rs`, `src/application/platform/host.rs` and this log.

`Session` point selection and every canvas-visible `PointView` now use the document model's stable `PointId`.
Tuple contour/point indices are confined to adapters around outline algorithms that still consume the compatibility glyph projection.
Canonical Project reloads retain selected IDs directly, while a legacy structural edit explicitly carries its temporary index selection only until the source guard reconciles and reloads the canonical layer.

The workspace clipboard now stores owned canonical `CopiedContour` values.
Copy reads the active `LayerView` and its stable selection, and paste appends fresh canonical contour and point identities through one guarded `CanonicalLayerTransaction`.
The Project owns the paste history step; the application stores only ordering context and reselects the returned `PastedContours::points` after the canonical reload.
The focused regression proves copied source points keep their identities, pasted points receive distinct identities, and Project-owned undo/redo removes and restores the pasted contour.

The Dimensions panel also reads `stem_and_bar_project` from the active canonical source rather than the compatibility font.
The direct-write `features --write` path now retains its established UFO-only format boundary while read-only Babelfont feature generation remains available.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::commands::tests::clipboard_paste_uses_canonical_contours_and_history -- --exact --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::session::tests -- --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::platform::host::tests -- --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The complete binary suite passed 167 tests with four documented model or external-font tests ignored.
All 15 focused session tests, 23 runnable host tests and the canonical clipboard regression passed.
Warning-denied workspace/all-target Clippy, formatting and diff checks passed.

## Canonical component, anchor and pointer-gesture slice

Implementation commit: `Move editor objects and gestures into canonical transactions`.
Resolve its exact ID with `git log --format=%H --grep='^Move editor objects and gestures into canonical transactions$' -1`.
Affected paths: `src/application/editor/session.rs`, `src/application/editor/commands.rs`, `src/application/editor/inspector.rs`, `src/application/platform/host.rs`, `src/application/view/canvas/editor.rs`, `src/application/view/panels/tabs.rs` and this log.

Component and anchor selection now use stable `ComponentId` and `AnchorId` values rather than tuple order or array indices.
The Session component cache now comes from `resolved_document_components`, so combined paint geometry, hit testing and selected-component feedback share the complete canonical smart-component, hyperbezier and metaball renderer.
Canonical resolved contour copies drive component decomposition, which commits through one guarded Project transaction.
Anchor add and delete also use guarded canonical layer edits.
The remaining legacy component duplication algorithm carries an index only until the canonical source reload assigns and selects the new stable identity.

Point, component and anchor pointer drags now own cloned `CanonicalLayerTransaction` values for the gesture lifetime.
Pointer motion mutates only the owned draft and a paint-time compatibility projection.
Pointer Cancel drops the transaction and restores the base projection, a no-op Pointer Up creates no history, and a changed Pointer Up hands exactly one labeled transaction to Project.
Application history retains only ordering context, and the legacy source history receives no point, component or anchor drag snapshot.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::session::tests -- --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::commands::tests::point_drag_cancel_noop_and_commit_use_one_canonical_history_step -- --exact --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::inspector::size_tests::anchor_drag_delete_undo_and_reopen -- --exact --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::platform::host::tests::component_add_move_undo_and_save_reopen -- --exact --nocapture
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

## Remaining application callers

- `Session` no longer stores a `norad::Glyph`, tuple point or anchor identities, `HistoryOp`, pending legacy snapshots or a second outline undo pile.
- The canvas, panels, browser state and inspector now read points, contours, components, anchors, metrics, Unicode, images, segment bounds, analysis geometry and metaball data through canonical views.
- Point, component, anchor, metric and metaball gestures own canonical transactions; cancel drops the draft, a no-op creates no history and Pointer Up publishes one transaction.
- Direct canonical draft operations now own filters, cleanup, transforms, shapes, anchors, images, boolean operations, knife cuts, curve conversion, re-interpolation and mask baking.
- Place Image installs bytes through a stable-source Project operation and attaches the image through the layer draft; it no longer mutates `FontModel::font_mut().images`.
- Overview metaball conversion now commits guarded layer transactions and replays Project-owned layer history instead of calling `FontModel::replace_glyph` or recording `Master.history` snapshots.
- The remaining temporary bridge callers are exact and finite: pen contour materialization and close; hyperbezier start, append and close; mark-label writes; compatibility contour replacement and its test-only paste helper; background send and swap; and the mark-cloud read projection.
- `FontModel` still exposes mutable source/font access for overview marks, Unicode, metrics formulas, local-model workflow boundaries, source retargeting and background layers; those callers remain M06/M13 work rather than completion claims.
- Pen, Shape and Knife still need explicit Pointer Cancel coverage in the final gesture audit.

## Canonical Session storage and history cutover

Implementation commit: `Remove the Session UFO cache and legacy history`.
Resolve its exact ID with `git log --format=%H --grep='^Remove the Session UFO cache and legacy history$' -1`.
Affected paths: `src/application/editor/session.rs`, editor commands and tools, canvas and panel readers, `src/application/font_model.rs`, application tests and this log.

`Session` now retains canonical layer transactions, stable object identities and presentation caches only.
There is no long-lived UFO glyph, legacy selection map, `HistoryOp` queue or application-to-Master whole-glyph synchronization path.
The application callback runs only after `sync_session_from` accepts and reloads a canonical transaction.
A stale transaction therefore cannot fall through to a later compatibility write, and a failed canonical reload retains a diagnostic and rejects later widget messages from that session.

The temporary bridge requested earlier was integrated as core commit `ed4ad11` and consumed only for short-lived algorithms that have not yet received a direct draft operation:

```rust
impl CanonicalLayerTransaction {
    pub fn compatibility_glyph(&self) -> norad::Glyph;
    pub fn reconcile_compatibility_glyph(
        &mut self,
        glyph: &norad::Glyph,
    ) -> Result<bool, DocumentEditError>;
}
```

This is migration debt, not a public editing architecture or an M13 completion claim.
Direct hyperbezier conversion, mask baking and stable-source image installation have already replaced three would-be bridge or mutable-font callers.

Executed evidence:

```sh
CARGO_BUILD_JOBS=1 cargo check --workspace --all-targets --locked
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender application::editor::session::tests:: -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --locked --bin runebender -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
git diff --check
```

The focused Session suite passes nine tests.
The complete binary suite passes 163 tests with four documented model or external-font tests ignored.
The rejected-transaction regression proves no fallback write, no dirty/history change and a retained reload diagnostic after the canonical glyph disappears before commit.

## Canonical no-op release and metaball-collapse slice

Implementation commit: `Preserve canonical no-op gestures and collapse metaballs`.
Resolve its exact ID with `git log --format=%H --grep='^Preserve canonical no-op gestures and collapse metaballs$' -1`.
Affected paths: `src/application/editor/session.rs`, `src/application/editor/commands.rs`, `src/application/editor/tools/metaballs.rs`, `src/application/platform/host.rs`, `src/application/view/canvas/editor.rs`, `src/application/view/panels/editor.rs` and this log.

`Workspace::sync_session_from` now reports changed, unchanged or rejected instead of reducing those states to one Boolean.
The real editor event dispatcher suppresses the `Edited` callback for an unchanged release, while non-edit events still reach the application callback.
An untouched release and an out-and-back point drag therefore leave document revision, dirty state, undo history and an existing redo step unchanged.
The panel refresh callback now finishes an already accepted transaction rather than synchronizing the same Session a second time.

Selected-glyph and whole-font metaball collapse now call `LayerEditDraft::collapse_metaballs` directly.
The overview command records Project-owned layer history for every changed glyph and no longer projects each glyph through the compatibility bridge.

The six tests removed with the legacy Session state were audited against current behavior:

- The master-pile history test is retired because editor outline history is Project-owned; point-drag, save-event and rejected-transaction regressions cover commit, persistence and stale rejection.
- Component selection, cache rebuilding and decomposition are covered by the host component lifecycle test and the nested-component decompose/undo regression.
- Component insertion validation is now asserted before the successful add in the host lifecycle test.
- Locking a loose component now has a host regression that proves immediate anchor snapping, later anchor realignment and ordered undo back to its unlocked transform.
- Canonical cleanup has an explicit `round_coordinates` draft regression.
- Duplicate Repeat has an application-level regression covering the retained last transform, fresh stable point selection, Project history and undo/redo.

Executed evidence:

```sh
cargo fmt --all --check
CARGO_BUILD_JOBS=1 cargo test --locked application:: -- --test-threads=1
git diff --check
```

All 166 runnable application tests passed with four documented model or external-font tests ignored.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

## Next action

Delete the finite bridge list above, replace remaining `FontModel::font_mut`, `master_mut`, `edit_sources` and legacy history callers with canonical Project operations, then remove the bridge itself during M13.
