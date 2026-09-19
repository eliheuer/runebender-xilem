# Canonical curve-conversion lane

Status: **implementation complete and focused validation green; integration pending**.

This worker lane started from integration checkpoint `b3c02dbbf0d95d91a8179ee922f94a975fa1226f` on branch `codex/babelfont-curve-conversion`.
It owns only canonical quadratic-to-cubic, cubic-to-quadratic and selected/all hyperbezier-to-cubic conversion.
The implementation is isolated in `src/document/babelfont/curve_conversion.rs` and is wired by one child-module declaration in `src/document/babelfont.rs`.
The dedicated regression fixture is `tests/canonical_curve_conversion.rs`.

## API contract

`LayerEditDraft::convert_quadratics_to_cubics` converts every ordinary quadratic segment in one canonical layer.
It elevates quadratics at `f64` precision without coordinate rounding.
Consecutive quadratic controls materialize their implied joins as newly identified cubic endpoints.
All-off-curve closed quadratics convert from the implied midpoint before the first stored control, preserving the closed shape and deterministic cyclic order.
Existing explicit endpoints, contour identities and unrelated geometry retain their document identities and source metadata.
Replaced quadratic controls are deliberately retired because one source handle is replaced by two cubic handles.

`LayerEditDraft::convert_cubics_to_quadratics` accepts a finite tolerance greater than zero.
It uses Kurbo's cubic-to-quadratic approximation and caps work at 1,024 output quadratics per source cubic.
Invalid tolerances, nonfinite output and approximation-limit failures reject the whole staged operation.
Existing cubic endpoints retain their identities and metadata, while replaced handles and intermediate endpoints receive fresh identities.
Lines, quadratics, components, anchors, metrics and layer metadata remain untouched.

`LayerEditDraft::convert_hyperbeziers_to_cubics` accepts stable point and contour selections.
When both selections are empty, it converts every hyperbezier contour.
Otherwise, it converts the union of directly selected contours and contours containing selected points.
Every supplied identity is validated before any conversion, including valid identities on ordinary contours.
The operation uses `Path::from_document_contour` and the existing spline solver without materializing a Norad glyph.
Existing hyper on-curve identities, names and libs survive, while solved handles receive fresh identities.
The stable contour identity and contour lib survive, while the legacy hyper-marking UFO identifier is retired and replaced only when the retained lib requires an identifier.

All three methods stage multi-contour work on an owned draft.
They return `Ok(false)` without mutating the draft when no eligible segment or contour exists.
An error leaves the original draft unchanged even when the caller catches the error and continues its transaction.
Project transaction commit therefore preserves revision and redo state for no-op conversions.

## Coordination

The proposed signatures were sent before implementation to integration lead task `01a0b7b7-4302-76c1-acde-bba76548faa4`, editor task `01a0ba61-b1cf-7541-853c-6558bae092d5` and orchestrator task `01a0b6c4-c0a7-7951-917f-7ed2ccda4428`.
The M06 editor owner confirmed the signatures fit the real command migration.
M06 will pass the session's stable point selection with an empty contour slice, keep tolerance `1.0` for the existing cubic-to-quadratic command and avoid recording no-op history.

## Validation

The implementation compiled with:

```sh
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo check --locked --lib
```

The dedicated suite passed four tests covering exact quadratic elevation, bounded cubic approximation, stable selected hyper conversion, preservation, history, no-op/error atomicity and UFO save/reopen:

```sh
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo test --locked --test canonical_curve_conversion -- --test-threads=1
```

The complete canonical variable-project regression suite passed 59 tests:

```sh
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo test --locked --test variable_project -- --test-threads=1
```

The existing legacy curve-conversion comparison remained green:

```sh
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo test --locked --lib outline::convert::tests::quad_cubic_conversions -- --exact --test-threads=1
```

The workspace passed strict Clippy and documentation generation:

```sh
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo clippy --workspace --all-targets --locked -- -D warnings
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo doc --workspace --no-deps --locked
```

`cargo fmt --all --check`, `bash .github/copyright.sh` and `git diff --check` also passed.
The dependency graph emitted the existing future-incompatibility notice for `block 0.1.6`; no curve-conversion warning or lint remains.
