# Babelfont canonical pipeline lane

Status: **active; M08 and M09 are not complete**.
Baseline: `fa6caca673fb28827d29e69fff8f7cf4e5b70183`.

## Completed commits

- `30c7592` (`Interpolate canonical layer geometry directly`) adds canonical `LayerView` compatibility checks, numeric extraction and owned interpolation results while preserving exact advances, named anchors, all six component coefficients, point roles and default-layer identities.
  Existing Project callers still compile through a labeled Norad compatibility adapter that now uses the canonical algorithm.
- `27b175f` (`Check compiler snapshot quantization`) replaces unchecked compiler casts for units per em, general metrics and kerning with finite, range-checked snapshot quantization.
  Editable `f64` inputs remain unchanged.
- `2e8d968` (`Normalize compiler metadata inputs`) routes groups, exact kerning pairs and glyph category inference through storage-neutral compiler helpers used by the existing snapshot path.
  Canonical metadata can feed the same helpers without a Norad round trip, and an unknown explicit OpenType category fails instead of being silently inferred.
- `2a38882` (`Preserve canonical shape order in interpolation`) makes compatibility and result construction retain the exact canonical contour/component paint sequence and contour closure in addition to geometry and object identities.
- `d5c2fcf` (`Integrate canonical source metadata ownership`) selectively integrates the lead-owned `SourceId` group and exact kerning owner plus immutable query required by the compiler lane.
- `4d2f9d6` (`Compile canonical groups and kerning directly`) removes compiler reads of source-font group and kerning projections.
  A dedicated pipeline regression proves unsaved fractional changes on default and non-default masters, one-revision invalidation, exact source retention, changed binary output and one quantized shaping application.
- `a8436aa` (`Read compiler glyph layers canonically`) reads default-layer codepoints and anchors through `Project::document_layer` for compiler codepoint output and category inference.
  The explicit OpenType-category lookup remains a deliberately isolated UFO boundary read until Project exposes the canonical source-glyph value.
- `7a71c59` (`Integrate canonical interpolation inputs`) switches Project interpolation and kerning interpolation to canonical layer and source-metadata queries.
  A Norad glyph is now materialized only after canonical interpolation for callers that still require the compatibility return type.
- `4f5ed1e` (`Type compiler glyph categories canonically`) makes category mapping consume `OpenTypeGlyphCategory` and explicitly rejects unrepresentable component, unassigned and unknown values.
  The temporary UFO lookup decodes once into that canonical type, so the forthcoming Project query can replace it without another compiler mapping path.
- `4ceb0e0` (`Keep HOI interpolation geometry canonical`) applies higher-order interpolation to canonical endpoint points and `InterpolatedLayer` before the compatibility output projection.
- `ccdf4cd` (`Compile canonical source glyph metadata`) removes compiler reads of the skip-export and OpenType-category UFO lib keys.
  Export and explicit category now come from `Project::document_source_glyph_metadata` by stable `SourceId`, with a dedicated compiler-snapshot regression.
- `68d7f23` (`Compile through canonical font info values`) makes compiler metadata adapters consume `CanonicalFontInfo` rather than Norad `FontInfo`.
- `62e4a6b` (`Compile canonical font info directly`) removes the temporary boundary decode and reads canonical names, UPM, metrics and OpenType values by stable `SourceId`.
  Its regression proves exact unsaved UPM retention, one compiler-only quantization, one revision, invalidation, changed output bytes and fresh compiled-cache publication.
- `cbb4c2b` (`Add canonical Designspace structure model`) adds stable axis, source, instance and rule identities; exact mapped locations; full and sparse source descriptors; checked structural edits; immutable compiler inputs; and an explicit checked Norad import/export boundary.
  The model accepts Project-assigned source and layer identities, preserves supported source ordering and metadata, and rejects unsupported cross-axis, discrete and anisotropic data.
- `1aeee24` (`Own canonical Designspace in Project`) installs that model in variable `Project` data and exposes `Project::document_designspace` plus an owned immutable `Project::compiler_structure` snapshot.
- `cf0f1a6` (`Test Project-owned canonical Designspace`) proves the Project-owned snapshot preserves mapped axes, interleaved sparse-source order, stable identities, instances and rules.
- `09de268` (`Compile canonical Designspace structure directly`) makes compilation consume canonical axes, full sources, sparse sources, instances and rules without reading the mutable legacy Designspace projections.
  The preview cache key is now the document revision plus the typed immutable `CanonicalCompilerStructure`, and a regression proves compilation still works after deliberately clearing the legacy structural projections while slider-only location changes reuse the compiled result.
- `c174466` (`Resolve compiler feature includes from source paths`) retains canonical source identity and semantics while resolving the source's persistence path by stable `SourceId`, so relative feature includes use the loaded UFO directory instead of the process directory.
  Dedicated tests prove both the snapshot path and compilation of a Designspace-relative include outside the repository.
- `246b2bb` (`Build native text inputs from canonical sources`) removes the native text tool's production reads of the active Norad font for glyph inventory, kerning and generated mark features.
  Canonical Project queries now supply exact advances, codepoints, UPM, groups, pairs, feature text, anchors and component-propagated anchors; boundary-parity tests retain the existing behavior.

## Executed checks

- `cargo test --locked --lib canonical_interpolation -- --test-threads=1`: 2 passed.
- `cargo test --locked --lib compiler_ -- --test-threads=1`: 2 passed.
- `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project interpolation_ -- --test-threads=1`: 2 passed.
- `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project source_authoring_keeps_identity_and_round_trips_the_designspace -- --exact --test-threads=1`: 1 passed.
- `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_compile -- --test-threads=1`: 9 passed.
- `cargo test --locked --test canonical_metadata -- --test-threads=1`: 8 passed.
- `cargo test --locked --test canonical_font_info -- --test-threads=1`: 3 passed.
- `cargo test --locked --test canonical_pipeline -- --test-threads=1`: 2 passed.
- `cargo test --locked --test canonical_designspace -- --test-threads=1`: 5 passed.
- `cargo test --locked --test compiler_include_path -- --test-threads=1`: 2 passed.
- `cargo test --locked --lib text::features::tests:: -- --test-threads=1`: 5 passed.
- `cargo test --locked --lib text::buffer::tests::canonical_project_builds_the_same_text_inputs_as_its_source_boundary -- --exact --test-threads=1`: 1 passed.
- `cargo clippy --locked --lib --tests -- -D warnings`: passed.
- `cargo fmt --all --check`: passed.
- `git diff --check`: passed.

The unchanged `block v0.1.6` future-incompatibility notice remains a dependency notice.
A broader `cargo test --locked --lib document::` run passed 76 tests before hitting 26 missing-adjacent-fixture failures and the known sandbox-denied Unix-socket test.
The affected tests were rerun with the documented fixture path above.
The focused HOI fixture test could not run through interpolation with the current external font checkout because canonical import rejects its pre-existing wrong-side `public.kern1.v` pair first; without that checkout the test reports the documented missing fixture.

## Integration dependencies

- `Project::interpolation_layers` now passes canonical `LayerView` values into `InterpolatedLayer` construction without materializing Norad inputs.
  Existing callers still require a final Norad return value, so `interpolation.rs` retains a labeled output projection after the canonical calculation.
- Canonical group and kerning storage now feeds compilation directly through `Project::document_font_metadata` and `CanonicalFontMetadata::{groups, kerning_pairs}`.
  `raw_kerning` remains confined to format-boundary materialization.
- Canonical layer-glyph metadata owns codepoints and notes, and compiler codepoints plus inferred categories read its `LayerView` projection directly.
  Canonical source-glyph metadata owns export and explicit category values, and compilation now reads them directly through the immutable Project query.
- Canonical font names, units per em, per-source metrics and current OpenType values now feed compilation directly through `Project::document_font_info`.
  Snapshot construction uses canonical Designspace source descriptors for names, locations and default-layer addresses.
- Axes, full sources, sparse sources, instances and rules now reach compilation through one owned immutable `CanonicalCompilerStructure` snapshot.
  The compiler cache no longer builds a debug-string fingerprint from `axes`, `master_locations`, `master_names`, `instances` and `ds_doc`.
- Native text-tool inventory, kerning fallback and generated mark-feature inputs now come from canonical Project queries.
  The Norad constructors remain for explicit source-boundary and fixture callers, not the production editor path.

## Remaining ownership and integration gap

The Project-owned compiler query and typed cache key are implemented.
M08 and M09 are still not complete because every source-authoring command, undo and redo path must mutate or restore that same canonical owner before compatibility projections can be considered non-authoritative.
The existing interpolation API also still projects canonical results back to Norad for application callers that have not yet accepted the canonical result type.
Those remaining caller cutovers require implementation and focused round-trip, invalidation and preservation proof; the presence of the canonical model and compiler query is not sufficient completion evidence.

## Next concrete step

Route source-authoring transactions and their undo/redo snapshots through the Project-owned canonical Designspace and prove that compiler invalidation follows those edits.
Retire the interpolation output projection when its remaining application and source-authoring callers consume canonical results directly.
