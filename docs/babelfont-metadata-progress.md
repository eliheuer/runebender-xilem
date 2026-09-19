# Babelfont canonical metadata lane

Status: **IN PROGRESS — font-level storage is integrated; glyph-level storage, history and application integration remain**.
M07 must not be marked complete until the M05 history and M06 application integrations pass the checklist acceptance criteria.

## Checkout

- Worktree: `/Users/eli/.codex/worktrees/78da/runebender-xilem`.
- Branch: `codex/babelfont-m07-metadata`.
- Required shared baseline: `fa6caca673fb28827d29e69fff8f7cf4e5b70183`.
- Adopted shared core checkpoint: `a90dd5d` (`Preserve hyper copies and port canonical filters`) through merge commit `24231aa`.
- The worktree was clean and detached at `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da`, then advanced non-destructively to the required baseline before the branch was created.
- No lead-owned Project, VariableData, Babelfont, source, application, shared integration-test, architecture, checklist, changelog or global progress file was edited.

## Completed slice

Evidence commit: `b814d6c` (`Add canonical font metadata values`).
Follow-up evidence commit: `709c6f2` (`Preserve canonical UFO glyph metadata`).
Group-operation commits: `4aad3aa` (`Make canonical group renames atomic`) and `3541f35` (`Reject ambiguous canonical group edits`).
Project integration regression: `d40ca12` (`Test canonical source metadata transactions`).
Glyph ownership split: `94ccd8a` (`Separate layer and source glyph metadata`).
Metrics formula move: `480ffe2` (`Move metrics formulas into canonical metadata`).

`CanonicalFontMetadata` now retains every UFO group and exact source-local `f64` kerning value without a Norad or Babelfont live model.
Kerning participants distinguish glyphs from side-specific groups, and pair construction rejects a group on the wrong side.
Resolution follows UFO precedence and distinguishes an explicit zero from a missing pair.
Pair and membership edits reject non-finite or invalid input before mutation.
Group removal, glyph-reference removal and glyph rename update all related group and pair references atomically while preserving unrelated groups.
The UFO boundary helpers decode and encode these values in one step, with no quantization.

`CanonicalLayerGlyphMetadata` now retains ordered unique Unicode values and an exact optional note at the layer that owns the GLIF fields.
`CanonicalSourceGlyphMetadata` retains the export flag and optional OpenType category for one glyph identity in one source.
`CanonicalGlyphMetadata` is only the UFO boundary transfer value that decodes and encodes both parts together; it is not intended as another live owner.
Known categories are typed, while an unknown source category remains an exact `Other(String)` for explicit downstream handling.
Unicode input accepts multiple hexadecimal values with `U+` or `0x` spelling and rejects an invalid scalar without changing the target glyph.
The existing Norad compatibility operation delegates Unicode parsing to that canonical parser.
The follow-up adds strict one-step UFO decoding and atomic encoding for Unicode, notes, export flags and categories while retaining unrelated font-lib entries.
Malformed standardized payloads are rejected before mutation, and an unchanged encode preserves list and dictionary ordering.
The [official UFO group contract](https://unifiedfontobject.org/versions/ufo3/groups.plist/) permits duplicate members in arbitrary groups and ignores later duplicate kerning-group members, so canonical import now preserves group order and duplicates exactly.
The [official lib key contract](https://unifiedfontobject.org/versions/ufo3/lib.plist/) defines `unassigned` alongside base, mark, ligature and component, and the canonical category type now represents it explicitly.
Group rename retains membership and rewrites every pair reference in one staged operation.
Whole-group edits reject adding a glyph to two kerning groups on one side while arbitrary groups remain free to overlap and retain duplicates.
Metrics formulas now live with canonical glyph metadata rather than in the UFO-format module.
The parser preserves hyphenated glyph names, recognizes rightmost finite arithmetic, rejects non-finite constants, exposes the referenced glyph and renames only matching references atomically.
Evaluation rejects non-finite referenced inputs or results, while constants remain independent of a reference value.

The lead's `a1f3d35` (`Own source groups and kerning canonically`) is adopted through merge commit `578c76e`.
`SourceMetadata` now owns the value by stable `SourceId`; the preserving UFO template no longer owns groups or kerning.
Project read and edit transactions report canonical metadata invalidation, while source snapshots and save materialize a temporary UFO boundary value.
The dedicated Project regression proves exact import, changed/no-op/rejected atomicity, compilation invalidation and save/reload persistence.

## Executed checks

The following checks passed in this worktree with `CARGO_BUILD_JOBS=2` for Cargo commands:

```sh
cargo test --locked --test canonical_metadata -- --test-threads=1
cargo test --locked --lib canonical_ufo_boundary_preserves_fractional_kerning_and_unrelated_groups -- --test-threads=1
cargo test --locked --lib document::model::glyph_metadata::tests -- --test-threads=1
cargo test --locked --lib outline::glyph_ops::tests -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The canonical integration suite now passes ten tests.
The focused font-metadata UFO boundary test passed one test, glyph metadata passed seven tests, the metrics-key parser passed one test, and the existing glyph-operation regression group passed eighteen tests.
Warning-denied test Clippy, public API documentation, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
An initial unit-test invocation combined incompatible repeated `--lib` flags and did not run.
A later exact-name filter matched zero tests; the corrected focused command above ran and passed the intended test.

## Integration dependencies and handoffs

The lead task `01a0b7b7-4302-76c1-acde-bba76548faa4` gave `variable::SourceMetadata` one `CanonicalFontMetadata` value per `SourceId`, removed groups and kerning from the preserving template, rehydrates them only at UFO export and exposes immutable Project reads plus `SourceMetadataEditDraft` mutation in `a1f3d35`.
The same request asks the lead-owned central rename transaction to change the canonical name index and component references while invoking `rename_glyph_references` for every source.
The lead reserved these hooks according to the coordinating task.

The M09 task `01a0ba2a-5670-7451-b05a-bd71293b2229` received the exact current read types and invalidation contract.
It owns `compile.rs` and `compile_metadata.rs` and will consume canonical values without creating another store.
Its compiler snapshot must quantize a copy, reject unsupported categories explicitly and never write quantized kerning back into the document.

## Next concrete step

The lead has integrated `b814d6c`, `709c6f2` and `4aad3aa`; hand it `3541f35`, the dedicated Project regression `d40ca12`, the glyph ownership split `94ccd8a` and the metrics formula move `480ffe2`.
The central glyph hook should store the layer value in `LayerPreservation` and the source value by `SourceId` in `VariableGlyph`, removing recognized glyph entries from the preserving templates while retaining unmatched names and opaque lib data.
The history lane has the `a1f3d35` API and is preparing guarded source-metadata history without UFO serialization.
After that M05 API lands, add canonical metadata snapshot replay tests covering no-op suppression, redo invalidation, source reorder and failed replay atomicity.
