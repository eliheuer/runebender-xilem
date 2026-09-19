# Babelfont canonical metadata lane

Status: **COMPLETE — canonical metadata, whole-glyph transactions, special-outline rendering and every M07 production caller are integrated**.
The full migration remains active in the integration lane for M13 compatibility-state removal and M14 final proof.

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
Source-metadata history import: `a42758c` (`Add canonical source metadata history`).
Multi-source history acceptance: `1bd8c80` (`Test multi-source metadata history`).
Special glyph data move: `f9e9eb3` (`Move special glyph data into canonical metadata`).
Legacy kerning compatibility: `21db42b` (`Preserve legacy wrong-side kerning references`).
Canonical font-info boundary: `3855fa7` (`Add canonical font info values`).
Canonical source-glyph owner: lead checkpoint `5d28144` (`Own source glyph metadata canonically`).
Canonical font-info owner: lead checkpoint `b3c02db` (`Own source font info canonically`).
Atomic font-info validation: lead checkpoint `1d004f0` (`Validate canonical font info transactions`).
Canonical compiler reads: lead checkpoint `3f20977` (`Compile canonical font info directly`).
Component-alignment value: `0b8e996` (`Type component alignment metadata`).
Component-alignment boundary cleanup: `f6b0edc` (`Route component alignment through canonical metadata`).
Font-info Project acceptance: `17a161c` (`Test canonical font info storage`) and `6430ad3` (`Test atomic font info transactions`).
Canonical layer metadata and component ownership: integration checkpoint `082e4f8` (`Own typed glyph-layer metadata canonically`) through merge commit `ecc4678`.
Canonical composition, effective-anchor, realignment and generated-feature queries: `f1b35ff` (`Plan composition and features from canonical layers`).
Deterministic duplicate-codepoint precedence: `21f2e0e` (`Preserve composition codepoint precedence`).
Typed Project composition planning: `5458407` (`Expose Project composition planning`).
Typed smart-component boundary codec: `86fdef7` (`Type smart component metadata`).
Atomic whole-glyph lifecycle transactions: `3a9fd68` (`Add atomic canonical glyph transactions`).
Guarded canonical composition proposal installation: integration checkpoint `7718d1c` (`Write composition proposals atomically`).
Proposal identity hardening: integration checkpoint `087e0e7` (`Retain proposal identities across reorder`).
Application glyph lifecycle and component-alignment cutover: integration checkpoint `2e16e09` (`Move application edits into canonical transactions`).
GUI and CLI composition cutover: integration checkpoint `36535d3` (`Write application composition proposals canonically`).
Headless composition, proof and analysis cutover: integration checkpoint `278d173` (`Run headless proof and analysis canonically`).
Canonical component selection rendering: integration checkpoint `1b5234e` (`Render canonical component selection paths`).
UFO-only direct feature-write boundary: application checkpoint `c6cd234` (`Keep direct feature writes UFO-only`).

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
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib document::composites::tests -- --test-threads=1
cargo test --locked --lib document::compose::tests -- --test-threads=1
cargo test --locked --lib text::features::tests -- --test-threads=1
# Standalone module harness pending the core-owned parent module declaration:
(cd /private/tmp/runebender-smart-components-check && cargo test --offline)
(cd /private/tmp/runebender-smart-components-check && cargo clippy --offline --all-targets -- -D warnings)
cargo test --locked --lib glyph_transactions::tests -- --test-threads=1
cargo test --locked --lib document::model::smart_components::tests -- --test-threads=1
cargo test --locked --lib outline::glyph_paths::canonical_render_tests -- --test-threads=1
cargo clippy --locked --lib --tests -- -D warnings
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --workspace --locked -- --test-threads=1
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --lib -- --test-threads=1
cargo clippy --locked --tests -- -D warnings
cargo doc --locked --no-deps
cargo fmt --all --check
git diff --check
```

The canonical metadata integration suite now passes thirteen tests, including the two multi-source history acceptance regressions and legacy wrong-side kerning preservation.
The canonical history suite passes thirteen tests, the canonical pipeline suite passes two tests and the variable compiler suite passes nine tests.
The focused font-metadata UFO boundary test passed one test, glyph metadata passed nine tests, the metrics-key parser passed one test, mark-color serialization passed three tests, metaball behavior passed six tests, and the existing glyph-operation regression group passed eighteen tests.
The component-alignment helper group passed six tests with the real font fixture path configured, and the canonical component transaction test passed independently.
The canonical layer-metadata transaction test passed independently.
The canonical font-info suite passed six exact boundary, default-resolution, Project ownership, metric-invalidation, history and failure-atomicity tests.
The composition suite passed eight tests, including direct canonical-plan equivalence, invalid typed-recipe rejection and repeated duplicate-codepoint precedence.
An independent external regression repeated the duplicate-codepoint plan 64 times against the legacy base and advance choice and passed.
The generated-feature suite passed five tests, including direct Project equivalence and the Project-level caller API adopted by the pipeline lane.
The composite suite passed nine tests, including canonical exact-transform retention and rejection atomicity.
The typed smart-component model passed seven codec tests after Project storage integration.
The canonical renderer passed eight path and bounds tests covering typed smart interpolation, nested special outlines, exact transforms, fallback, non-finite rejection and recursion limits.
The canonical glyph-transaction group passed eleven tests covering identity, exact layer cloning, partial-source insertion, multi-source add-missing, duplicate, rename/remove references, Project history, no-op, stale rejection and source save/reopen; warning-denied library and test Clippy also passed.
The earlier real-font workspace run passed 383 library tests, 165 binary tests with four intentionally ignored model or large-fixture tests and every integration suite.
After the combined smart-renderer and whole-glyph merge, the focused variable-project suite passed 62 tests.
The first integrated workspace run exposed a removed-source compatibility-history regression; core checkpoint `b2c5511` retained that temporary history outside canonical snapshots, M05 accepted it independently, and the complete rerun passed.
The final combined library run passed all 445 tests, including canonical composition callers, proposal identity retention, sparse-source and instance transactions, brace interpolation, headless proof, glyph analysis and the Unix live socket.
The sandboxed form of that run passed 444 tests and failed only because the live-socket test could not create its endpoint; the authorized non-sandboxed rerun passed all 445.
The final integrated workspace run passed those 445 library tests, all 166 enabled binary tests, all integration suites and documentation tests; four model or large-fixture binary tests remained intentionally ignored.
That run also caught and verified the repair for direct `features --write` against an imported Babelfont package, which must remain UFO-only and leave the package unchanged.
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

## Handoff

The integration branch adopted `86fdef7` as the single typed owner of the three smart-component source keys and the canonical renderer over those Project queries.
The guarded proposal transaction now owns M07's immutable composition payloads and retains stable object metadata across reorder.
The five `FontModel` glyph-lifecycle commands, component-alignment command, GUI and CLI composition commands and `core.compose` node all use canonical Project transactions.
No production caller remains in the M07 ledger.
Removal of now-unused compatibility state belongs to M13 and must not be confused with an unfinished M07 caller cutover.
