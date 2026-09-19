# Babelfont editor/application migration lane

Status: **ACTIVE — application ownership and canonical transaction integration in progress**.

This lane owns M06 application work under `src/application/`, focused application tests and this progress record.
The integration lane retains the canonical Project implementation, shared module wiring and central migration documents.
Parallel authorization changes lane ownership only; the migration checklist's preservation and completion requirements remain unchanged.

## Baseline and scope

The clean isolated worktree was advanced from `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da` to integrated commit `0b06ad2` and placed on `codex/babelfont-editor-m06`.
This lane owns editor sessions, application commands, `FontModel`, workspace state, canvas and panel presentation, platform session synchronization and browser application bootstrap when directly required by M06.
M10 and M11 proposal, live-tool, Nodes and headless workflow migrations remain outside this lane except for unavoidable compiling adapters.

## Required shared APIs

M06 has requested Project-owned canonical history wrappers from the core and history lanes.
The required surface records and coalesces completed `GlyphLayerAddress` transactions, discards a no-op group, queries undo/redo and replays guarded history while returning invalidation information.
Session will not own another undo pile.

M06 has also requested an owned guarded canonical layer transaction for the editor island.
It must begin from a `GlyphLayerAddress`, expose the existing `LayerEditDraft` read/write surface, retain the exact base state and commit only when the addressed live layer still matches that base.
This lets the canvas preview a pointer gesture locally and commit once without retaining a mutable Norad glyph or reconciling a complete source font after input.

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
Warning-denied binary Clippy, formatting and diff checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.

## Remaining application callers

- `application/editor/session.rs` still stores and mutates a `norad::Glyph`, resolved Norad component contours and pending Norad history records.
- `application/font_model.rs` still exposes mutable `Master` and `norad::Font` accessors and performs font-wide edits through compatibility projections.
- `application/workspace.rs` still stores a Norad contour clipboard and application-owned rename, Unicode and overview history values.
- `application/editor/commands.rs`, inspector and tools still call legacy font/source mutation APIs.
- `application/view/canvas/editor.rs` still paints handles and anchors from the session's Norad glyph.
- Several panels still read compatibility font and glyph metadata pending the M07 canonical metadata surface.

## Next action

Migrate the session and canvas to the guarded canonical layer transaction as soon as the shared Project boundary lands.
In parallel, move independent read-only render/cache paths to `Project::document_layer` and stable source/layer identities without changing UI behavior.
