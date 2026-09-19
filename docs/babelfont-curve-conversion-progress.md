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

## Integration handoff

The tested implementation commit is `54253fbe59d543b12f81f67712ed6f9e0ecfd788`.
It was sent to the integration lead, M06 editor owner and orchestrator without a push or merge from this worker.
The integration lead reported that its current turn was exhausted and explicitly made no acceptance claim.
An independent follow-up review subsequently passed three cyclic-start tests across 16 starting positions for quadratic chains, exact cubics and closed hyperbezier geometry using the built library artifact.
That review also verified closure and unique document identities; it filtered a sub-`1e-9` implicit closing-line artifact in its comparison oracle rather than requesting a product change.
The review artifact is `/private/tmp/runebender-migration-review.porJgX/curve_conversion_cyclic_review.rs`.
Core has supplied acceptance and a handoff for integration, but this worker has not claimed or performed the integration itself.

## Canonical handle-cleanup continuation

Core transferred the bounded round-corners, harmonize, balance and optimize slice to this existing worker lane after the curve-conversion review.
The implementation lives in `src/document/babelfont/handle_cleanup.rs`, with dedicated coverage in `tests/canonical_handle_cleanup.rs`.

`LayerEditDraft::round_selected_corners` validates every selected stable point identity before staging work.
It handles selected interior line-line corners on open contours and cyclic corners on closed contours while skipping hyperbezier contours and ineligible selections.
The original corner identity and its name/lib metadata move to the incoming fillet endpoint.
The second endpoint and two cubic handles receive fresh identities, and the operation returns the replacement stable point selection required by the editor.
Untouched points, contours, components, anchors, advances and layer metadata remain exact.

The round-corners checkpoint passed three dedicated tests covering open and closed contours, replacement selection, metadata and identity preservation, atomic no-op/error behavior, canonical undo/redo and UFO save/reopen:

```sh
CARGO_TARGET_DIR=/private/tmp/runebender-curve-conversion-target CARGO_BUILD_JOBS=1 cargo test --locked --test canonical_handle_cleanup -- --test-threads=1
```

`LayerEditDraft::harmonize_handles` preserves the current command's selected-smooth-node scope, with an empty selection considering every eligible join.
It changes only the two handles adjacent to a smooth join between closed cubic segments, retains their identities and metadata, and leaves open, hyperbezier, degenerate and ineligible mixed-curve joins untouched.
It uses the shared `analysis::curve::harmonize` primitive and preserves the command's integer-grid result without materializing a UFO glyph.

Two harmonize regressions brought the dedicated suite to five passing tests.
They cover exact stable-ID handle movement, selection scope, canonical undo and caught-error/no-op atomicity.

`LayerEditDraft::balance_handles` puts a cubic segment in scope when any of its four stable point identities is selected, with an empty selection considering every eligible segment.
It uses the shared `analysis::curve::balance` primitive, preserves the command's rounded result, and moves only the two existing handles.
Open, hyperbezier, degenerate and non-cubic segments remain untouched, while all surviving point identities and metadata remain exact.

Two balance regressions brought the dedicated suite to seven passing tests.
They cover segment selection through a handle identity, exact shared-primitive output, source metadata preservation and no-op history behavior for open contours.

`LayerEditDraft::optimize_handles` validates finite nonnegative tolerance before staging and retains the editor's whole-contour selection scope.
An empty selection considers all eligible closed ordinary contours.
It calls the shared `analysis::curve::optimize_contour` primitive, but writes back only handles proven to belong to explicit cubic segments.
Quadratic controls on mixed contours therefore remain exact instead of being silently reinterpreted as cubic handles.
Open and hyperbezier contours also remain untouched, while stable identities and source metadata stay attached to surviving points.

Three optimize regressions bring the dedicated suite to ten passing tests.
They cover empty-selection scope, exact shared-primitive output, stable identities and metadata, canonical undo, selected mixed contours, preservation of open, hyperbezier and quadratic geometry, valid zero tolerance, and caught invalid-parameter/error atomicity.

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
