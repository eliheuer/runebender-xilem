# Babelfont migration final proof

Status: **COMPLETE** for the editing-model migration.
Validation date: 2026-09-19.
The tested implementation is `5bb0ccc61d45944ed2a17c6ed5c18f255a5ec7f5`, tree `319005549d7d74e8e4ba323f32dd69627f455cd6`.
The following completion commit changes documentation only; it does not alter the tested Rust, manifests, lockfiles or browser sources.
This closes M14 in [the migration checklist](babelfont-migration-checklist.md).

## Candidate and independent review

The implementation worker handed off clean M13 commit `6401eeef99aa2ea51771827d34e6cc84e93f3538` and stopped changing it.
Orchestration cloned the repository into `/private/tmp/runebender-final-migration-20260919/candidate` and reviewed the final changes against testing checkpoint `9f55a21`.
The clone uses the pinned toolchain and locked dependencies without committed or untracked Cargo path patches.
Existing native and browser build caches were reused with two and one Cargo jobs respectively.
The browser's documented `web/prepare.py` generated its ignored pinned widget adapter.

The review followed the production source-format boundaries, canonical layer transactions, preservation records, history, compilation, live refresh and proposal paths.
There is no editable Norad source or glyph mirror and no whole-source reconciliation in ordinary layer commits.
`begin_document_layer_transaction` captures the addressed layer; commit and history compare and restore canonical layer snapshots.
Glyph-free source-format records clear canonical fields and contain no glyph payloads.
The [production inventory](babelfont-migration-inventory.md) and [preservation contract](source-format-allowlist.md) describe the retained codecs.
All four AST architecture tests passed, including production code after test modules and non-test cfg expressions.

The first full run found a real application defect: undoing an added component pruned its selection, and redo restored geometry without the selection expected by the existing component test.
Commit `5bb0ccc` records before/after stable component selection in application history references and restores it after canonical replay.
Project still owns geometry and its history.
The existing add/move/save test and extended duplicate/delete undo/redo assertions pass.
Initial failure logs are retained separately from the corrected run.

## Executed native checks

Environment: Apple silicon, macOS 26.6.2, Rust/Cargo 1.96.1, wasm-bindgen 0.2.127.
All of these commands passed in the clean candidate:

```sh
git diff --check
cargo fmt --all --check
bash .github/copyright.sh
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked -- --test-threads=1
cargo doc --workspace --no-deps --locked
cargo build --workspace --release --locked
cargo deny --locked check advisories
```

The full suite executed **805 passing tests**: 447 library tests, 177 application tests and 181 integration tests.
This includes canonical history, source history, Designspace structure, metadata, variable compilation, CLI, Unix live IPC, proposal/experiment and preservation coverage.
Four application tests were ignored by that command.
The two real-font tests were then explicitly executed with `--ignored virtua --test-threads=1`, adding **two passes** for disposable edit/save/reopen and mixed Arabic/Hebrew/Latin shaping.
The two local-model tests were not executed and are not counted as coverage.
The existing `block v0.1.6` future-incompatibility notice remains; the advisory check reported `advisories ok`.

## Browser and visual checks

`python3 web/prepare.py`, `sh web/build.sh`, and strict locked WASM Clippy passed with the documented SIMD flags.
`web/quality.cjs` passed at device pixel ratios 1, 2 and 1.25, including actual point edits, undo/redo, compiled export changes, themes, zoom, panel resizing, DOM composition/paste, Nodes, viewport resizing and idle behavior.
The measured frame medians/p95 values were 3.9/5.5 ms at 1x, 8.8/11.2 ms at 2x, and 5.1/6.8 ms at 1.25x.
These are this harness run's samples, not a general interactive performance guarantee.

Gray and Light browser captures were inspected after 3.2 seconds of idle time.
Gray and Light native captures were inspected for ordinary A editing, the Regular/Bold source controls, a weight-550 interpolated outline and compiled text proof, and the Layers controls.
The native renderer advances its idle animation frame before capture.
An additional isolated fixture copied A into the otherwise empty background layer for the layer-control captures.
The current Layers panel exposes name-based copy/remove and undo/redo controls, not a list or canvas selector for arbitrary auxiliary layers.
Canonical auxiliary-layer behavior is covered by the integration tests; the screenshots do not claim a richer UI workflow.

The browser uses its bundled in-memory font; arbitrary UFO/Designspace import and persistent source saves remain desktop workflows.
These checks do not certify native OS input methods, accessibility, GPU behavior, Windows/Linux runtime behavior, Safari or Firefox.

## Disposable Virtua Grotesk workflow

The probe loaded a copied two-source family with 863 document glyphs.
It changed A's point and advance through a canonical transaction, checked no-op revision/history behavior, and verified exact undo/redo restoration.
It compiled the unsaved edited document before any source save, exported a variable TTF, reused the current compiled snapshot, and checked distinct outlines and valid advances at normalized locations 0, 0.5 and 1.
Latin, Hebrew and Arabic shaping was exercised at all three locations, including Arabic contextual forms and the variable `a.bold` substitution.
The generated TTF has 846 exported glyphs and is 141,700 bytes.

Save/reopen and Save As/reopen preserved the complete supported UFO values for both sources: metainfo, font info, layers and glyphs, lib, groups, kerning, feature text, images and data.
Save As left the previous document unchanged.
The preservation allowlist and filesystem tests additionally cover custom GLIF paths, opaque bytes, staged publication and external feature dependencies.
The before/after SHA-256 manifest confirms that all **3,034 original Virtua source files remained byte-for-byte unchanged**.
Semantic equality after serialization is distinct from byte equality of untouched originals.

## Document performance and memory

The optimized standalone probe loaded the same two-source, 863-glyph disposable family.
After 32 warm-ups it measured 512 width transactions and their undos, without saving, compiling or rendering.
It restored the original layer and retained bounded history of zero undo entries and one redo entry.

| Operation | Median | 95th percentile | Maximum |
|---|---:|---:|---:|
| Canonical width transaction | 0.762 ms | 0.780 ms | 0.892 ms |
| Canonical undo | 1.512 ms | 1.535 ms | 1.693 ms |

`ps` reported 8,592 KiB RSS before load, 53,680 KiB after load and 53,856 KiB after the run.
`/usr/bin/time -l` independently reported a maximum resident set size of 55,214,080 bytes, approximately 52.7 MiB.
This is one document-operation measurement, not total editor memory, UI latency, a scaling bound or a comparison with an unmeasured baseline.
Source inspection, rather than timing alone, verifies that the whole-source reconciliation path was removed.

## Evidence and remaining product work

Local scripts, logs, counts, environment details, hashes, screenshots, exported font and preserved native executable are under `/private/tmp/runebender-final-migration-20260919`.
The native executable's SHA-256 is `e853065ea62ba0c972743654966af5c7c34eab7e2e3b4f2ca6a3c72ab73c4d88`.
`candidate-evidence/attempt-6401eee` preserves the initial candidate's results.
The standalone probe initially omitted the repository's thin-LTO flag, causing Apple's linker to reject newer LLVM bitcode; adding the matching Rust LTO flags fixed the harness without changing application code.
The corrected workflow, performance and visual runs all exited successfully.
No third-party font source or model was committed as part of this report.

The migration is complete within the recorded source-format contract.
[Known limitations](known-limitations.md) still apply, including unsupported Designspace extensions, browser persistence, model-dependent validation and broader native platform testing.
New agentic editing work should build on the live canonical Project and preserve this architecture and regression coverage.
