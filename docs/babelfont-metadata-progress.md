# Babelfont canonical metadata lane

Status: **IN PROGRESS — typed M07 values and algorithms are ready; canonical storage, history and application integration remain**.
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

`CanonicalFontMetadata` now retains every UFO group and exact source-local `f64` kerning value without a Norad or Babelfont live model.
Kerning participants distinguish glyphs from side-specific groups, and pair construction rejects a group on the wrong side.
Resolution follows UFO precedence and distinguishes an explicit zero from a missing pair.
Pair and membership edits reject non-finite or invalid input before mutation.
Group removal, glyph-reference removal and glyph rename update all related group and pair references atomically while preserving unrelated groups.
The UFO boundary helpers decode and encode these values in one step, with no quantization.

`CanonicalGlyphMetadata` now retains ordered unique Unicode values, an exact optional note, the export flag and an optional OpenType category.
Known categories are typed, while an unknown source category remains an exact `Other(String)` for explicit downstream handling.
Unicode input accepts multiple hexadecimal values with `U+` or `0x` spelling and rejects an invalid scalar without changing the target glyph.
The existing Norad compatibility operation delegates Unicode parsing to that canonical parser.
The follow-up adds strict one-step UFO decoding and atomic encoding for Unicode, notes, export flags and categories while retaining unrelated font-lib entries.
Malformed standardized payloads are rejected before mutation, and an unchanged encode preserves list and dictionary ordering.
The [official UFO group contract](https://unifiedfontobject.org/versions/ufo3/groups.plist/) permits duplicate members in arbitrary groups and ignores later duplicate kerning-group members, so canonical import now preserves group order and duplicates exactly.
The [official lib key contract](https://unifiedfontobject.org/versions/ufo3/lib.plist/) defines `unassigned` alongside base, mark, ligature and component, and the canonical category type now represents it explicitly.

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

The canonical integration suite passed seven tests.
The focused font-metadata UFO boundary test passed one test, glyph metadata passed six tests, and the existing glyph-operation regression group passed eighteen tests.
Warning-denied test Clippy, public API documentation, formatting and whitespace checks passed.
The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
An initial unit-test invocation combined incompatible repeated `--lib` flags and did not run.
A later exact-name filter matched zero tests; the corrected focused command above ran and passed the intended test.

## Integration dependencies and handoffs

The lead task `01a0b7b7-4302-76c1-acde-bba76548faa4` was asked to give `variable::SourceMetadata` one `CanonicalFontMetadata` value per `SourceId`, remove groups and kerning from the preserving template after promotion, rehydrate them only at UFO export, and expose immutable Project reads plus `SourceMetadataEditDraft` mutation.
The same request asks the lead-owned central rename transaction to change the canonical name index and component references while invoking `rename_glyph_references` for every source.
The lead reserved these hooks according to the coordinating task.

The M09 task `01a0ba2a-5670-7451-b05a-bd71293b2229` received the exact current read types and invalidation contract.
It owns `compile.rs` and `compile_metadata.rs` and will consume canonical values without creating another store.
Its compiler snapshot must quantize a copy, reject unsupported categories explicitly and never write quantized kerning back into the document.

## Next concrete step

Hand `b814d6c` followed by `709c6f2` to the lead for selective integration and adjust only the small module path or accessor names it requests.
Once the lead's reserved source-metadata hooks land, add Project-level exact group and kerning read/edit regressions plus save-reopen proof without editing its central files independently.
After the M05 history API lands, add canonical metadata snapshot replay tests covering no-op suppression, redo invalidation, source reorder and failed replay atomicity.
