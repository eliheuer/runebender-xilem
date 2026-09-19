# Babelfont headless and analysis lane

Status: **ACTIVE — canonical source selection, feature generation and direct analysis are integrated; proposal, compare, proof and glyph-inspection caller cutovers remain**.

This lane owns `src/document/nodes_run.rs`, `src/analysis/`, focused tests and this progress record.
The core integration lane owns `Project`, canonical payload installation, module wiring and the central migration checklist.
M06 owns application callers.
M07 owns composition, component alignment and generated-feature algorithms.
M10 owns proposal, edit-batch, experiment and live-tool operations.

## Integrated work

`analysis::dimensions::stem_and_bar_from_layer` measures a canonical `LayerView` directly.
It retains the established threshold, stem and bar behavior, including empty-glyph results.

`analysis::curve::cubics_from_layer` exposes the canonical curve-analysis entry point.
The Norad entry point remains only as a compatibility boundary while M06 moves the remaining application session caller.

Headless source and master selection now load `Project` and read stable `SourceId` values through `document_sources`.
Nodes that require one physical source reject a multi-source document explicitly instead of selecting one implicitly.

`core.features` now generates anchor classes and mark positioning from canonical layers with `features::generate_project`.
Its `RunValue::Path` output and JSON report fields remain unchanged.
Writing `features.generated.fea` and its include line remains an explicit filesystem boundary.

## Required behavior retained

The `RunValue` tagged JSON schema remains unchanged.
Node success and failure still flow through the existing graph report and process exit contract.
Source selection remains by exact display name, with the first source used only at the explicit `core.source` boundary when no name was supplied.
Layer selection remains exact and case-sensitive.
Foreground-writing nodes continue to require an explicit install operation, and proposal-only validation continues to reject them.
Overwrite refusal, cache fingerprints and progress events remain unchanged.

## Remaining caller cutovers

`core.compose` must apply M07's canonical composition plan through the guarded canonical proposal-layer writer.
It must not save a temporary Norad font.

`core.install` must call M10's `Project` proposal installer and save through `Project`.
It must retain structure checking, selected-glyph filtering and the existing `Installed` JSON report.

`core.compare` must compare canonical layer views by stable source and layer identity.
It must retain point-structure refusal, the unchanged and mean-shift baselines and per-glyph explanation rows.

`core.proof` must enumerate and render canonical layers with component resolution.
It must retain explicit layer selection, default drawn-glyph filtering and the eight-column SVG proof contract.

`analysis::glyph` must read canonical layers and component-resolved bounds.
Its revision token must continue to use the exact `glif-sha256:` codec contract until all batch clients migrate together.

`add_interpolated_source` must install the canonical interpolation result into the new stable source identity.
The UFO written at the final persistence boundary must remain an output adapter, not the editing model used to create the source.

## Focused evidence

The following checks passed on this lane after the source-selection and feature-generation cutovers:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::nodes_run::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib text::features::tests::canonical_generation_matches_the_legacy_source_projection -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::composites::tests::canonical_alignment_matches_legacy_and_preserves_exact_linear_transform -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::compose::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --lib -- -D warnings
cargo fmt --all --check
git diff --check
```

The nodes-run suite passed five tests.
The canonical feature parity check passed.
The canonical component-alignment check passed.
The composition suite passed six tests.
Warning-denied library Clippy, formatting and whitespace checks passed.

These are focused lane checks, not the final integrated migration proof.
The final proof must run after every caller above is canonical and after the core integration lane has assembled all migration lanes.
