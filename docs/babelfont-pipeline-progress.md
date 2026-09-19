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
  The model accepts Project-assigned source and layer identities, preserves supported source ordering and metadata, rejects unsupported cross-axis, discrete and anisotropic data, and is not yet installed as Project's authoritative structural owner.

## Executed checks

- `cargo test --locked --lib canonical_interpolation -- --test-threads=1`: 2 passed.
- `cargo test --locked --lib compiler_ -- --test-threads=1`: 2 passed.
- `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project interpolation_ -- --test-threads=1`: 2 passed.
- `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_project source_authoring_keeps_identity_and_round_trips_the_designspace -- --exact --test-threads=1`: 1 passed.
- `RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources cargo test --locked --test variable_compile -- --test-threads=1`: 9 passed.
- `cargo test --locked --test canonical_metadata -- --test-threads=1`: 8 passed.
- `cargo test --locked --test canonical_font_info -- --test-threads=1`: 3 passed.
- `cargo test --locked --test canonical_pipeline -- --test-threads=1`: 2 passed.
- `cargo test --locked --test canonical_designspace -- --test-threads=1`: 4 passed.
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
  Snapshot construction also uses `SourceView` for source names, locations, paths and default-layer addresses.
- Axes, instances, rules and brace-source structure still use the current Project fields or Designspace preservation document.
  They remain M08 ownership prerequisites for the final M09 architecture.

## Pending canonical structural query contract

The compiler lane has requested one Project-owned immutable boundary for the remaining structural inputs.
It must expose ordered canonical axes with names, tags, user/design bounds and mappings; ordered instances with stable identity, names and locations; canonical substitution rules independent of the mutable Designspace preservation document; and sparse/intermediate source descriptors that map layer addresses to their compile locations.
The same boundary must provide a canonical structural revision or fingerprint that is guaranteed to change when any of those inputs changes.
That value will replace the compiler cache's debug-string fingerprint of `axes`, `master_locations`, `master_names`, `instances` and `ds_doc`.

The typed value and codec boundary now exist in `document::model::designspace`, but Project does not own or query that value yet.
Core must install it as the single structural owner and route existing source transactions through it before the compiler consumes `CanonicalCompilerStructure`.
The compiler lane will not add a second structural store or read editable compatibility projections to simulate that integration.

## Next concrete step

Add source-glyph compilation invalidation coverage once its canonical edit transaction lands.
Consume canonical axes, instances, rules and brace-source structure as their M08 Project queries land.
Retire the interpolation output projection when its remaining application and source-authoring callers consume canonical results directly.
