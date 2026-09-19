# Babelfont headless and analysis lane

Status: **ACTIVE — every lane-owned headless operation is canonical; the application glyph-inspection caller handoff remains with M06**.

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

`Project::add_interpolated_source` now stages every glyph through canonical interpolation and M07's canonical layer-clone factory.
It installs the complete source, source metadata and Designspace replacement in one guarded structural transaction.
The new source retains each logical `GlyphId` while assigning fresh layer-object identities for contours, points, components and anchors.
Feature text, glyph export metadata, groups, interpolated exact kerning, typed font information and UFO preservation resources are carried through canonical owners.
The Norad `Master` is materialized only after the canonical commit as a compatibility and persistence projection.

`core.compose` now builds M07's canonical composition plan and writes it through M10's guarded Project proposal transaction.
It saves only the resulting proposal layer and never installs into the foreground implicitly.

`core.install` calls the canonical Project proposal installer and saves through Project.
`core.compare` reads exact canonical source and proposal layers while retaining structure refusal, unchanged and font-wide mean-shift baselines.

`core.proof` enumerates canonical layers and renders every path through `Project::document_layer_path`.
Selected-layer component resolution falls back to the source default while typed smart poles and metaballs remain part of the rendered result.
Default drawn-glyph filtering and the existing eight-column SVG and metrics contracts are retained.

`analysis::glyph::read_project_glyph` reads metrics, contours, components, anchors and component-resolved bounds directly from canonical Project layers.
Its output matches the existing JSON contract and continues to emit the exact `glif-sha256:` revision codec used by batch clients.

## Required behavior retained

The `RunValue` tagged JSON schema remains unchanged.
Node success and failure still flow through the existing graph report and process exit contract.
Source selection remains by exact display name, with the first source used only at the explicit `core.source` boundary when no name was supplied.
Layer selection remains exact and case-sensitive.
Foreground-writing nodes continue to require an explicit install operation, and proposal-only validation continues to reject them.
Overwrite refusal, cache fingerprints and progress events remain unchanged.

## Remaining caller handoff

M06 must route the CLI and live application entry points through `analysis::glyph::read_project_glyph` after this lane is integrated.
The UFO-boundary wrapper remains until those application callers move together.

## Focused evidence

The following checks passed on this lane after the source-selection and feature-generation cutovers:

```sh
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::nodes_run::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib analysis::glyph::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib text::features::tests::canonical_generation_matches_the_legacy_source_projection -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::composites::tests::canonical_alignment_matches_legacy_and_preserves_exact_linear_transform -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --lib document::compose::tests -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo test --locked --test variable_project -- --test-threads=1
RUNEBENDER_TEST_FONTS=<font-fixtures> cargo test --locked --lib -- --test-threads=1
CARGO_BUILD_JOBS=2 cargo clippy --locked --lib -- -D warnings
CARGO_BUILD_JOBS=2 cargo clippy --locked --test variable_project -- -D warnings
cargo fmt --all --check
git diff --check
```

The nodes-run suite passed eight tests, including canonical composition proposal and selected-layer proof parity.
The canonical glyph-inspection JSON parity check passed.
The canonical feature parity check passed.
The canonical component-alignment check passed.
The composition suite passed eight tests.
The variable-project suite passed 61 tests, including canonical source creation, fresh object identity, exact metadata and resource preservation, one-step revision history, error atomicity, undo/redo and save/reopen.
With the external font fixtures configured, 435 library tests passed in the sandbox.
The sole Unix-socket test was denied temporary IPC creation by the sandbox and passed when rerun outside it, completing all 436 library tests.
Warning-denied library Clippy, formatting and whitespace checks passed.
Warning-denied Clippy also passed for the variable-project integration target.

These are focused lane checks, not the final integrated migration proof.
The final proof must run after every caller above is canonical and after the core integration lane has assembled all migration lanes.
