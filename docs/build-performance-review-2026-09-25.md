# Build performance review, 25 September 2026

This section records the original baseline review.
The implementation follow-up below records changes subsequently authorized and applied.

Runebender's largest measured build bottleneck is the application binary compilation and link, not the Babelfont crate.
Replacing Babelfont alone would leave the direct `fontc` dependency, most shared font dependencies, and the large Xilem application compilation intact.
There are smaller, more direct improvements worth making before a font-model replacement.

The original review was read-only.
Experiments ran in an archived copy of commit `73d5968084e3272058027e4b44327b3fe986065c` under `/tmp/runebender-build-review`.
At that stage, the working checkout's application, manifests, dependencies, and build script were not changed.

## Measurements

The host is an Apple M4 Pro with 12 CPU cores and 48 GiB of physical memory.
Cargo used 12 jobs and Rust 1.96.1 from the repository's `1.96` toolchain.
Builds ran sequentially, offline, with the committed lockfile and already downloaded dependency sources.
Each clean comparison used an empty target directory; these are compilation measurements, not download or installation measurements.
Each comparison is one run, not a statistical benchmark.

| Measurement | Existing development profile | `debug = "line-tables-only"` |
|---|---:|---:|
| Clean native build | 114.4 s | 107.1 s |
| Application binary unit, including link | 75.07 s | 71.49 s |
| Font-engine library unit | 9.25 s | 8.39 s |
| Application invocation resource high-water mark | 10.31 GiB | 10.31 GiB |
| Fresh target directory, allocated disk space | approximately 6.0 GiB | approximately 4.1 GiB |
| Small UI label edit and rebuild | 7.07 s | 5.09 s |

Reduced debug information saved approximately one third of fresh target disk space and 6% of clean wall time in these runs.
It did not materially change the high application-unit memory reading.
An additional baseline rebuild with cached dependencies and freshly cleaned Runebender artifacts independently sampled process RSS approximately every 100 milliseconds.
The application rustc process reached 9.96 GiB, the linker reached 1.51 GiB, and the maximum simultaneous RSS sum for the monitored build tree was 10.08 GiB.
Sampling can miss short peaks, and summed RSS can double-count shared pages, but it independently confirms that the approximately 10 GiB problem occurs in rustc itself.
The linker was not the dominant memory consumer in this run.
It also removes type and variable information from debugging; retain an explicit full-debug option for debugger work.
The relevant Cargo settings and tradeoffs are documented in the [Cargo profile reference](https://doc.rust-lang.org/cargo/reference/profiles.html).

The small edit changed the welcome label in `src/application/view/render.rs`.
This is one incremental workload; an engine edit, a large type change, a dependency update, and a test build can behave differently.

### An avoidable rebuild

`build.rs` reserves stack space for Windows, but declares no `rerun-if-changed` inputs.
The ordinary UI-label edit consequently reran the build script and recompiled both the font library and application.
In the temporary copy, adding `println!("cargo::rerun-if-changed=build.rs");` and then making another label edit resulted in only the binary being recompiled.
That rebuild took 2.70 seconds instead of the earlier 7.07 seconds.
The library compiler invocation disappeared from the recorded invocations.
This is a small, concrete first fix; validate Windows build-script behavior before promotion.

### Application type complexity

`src/application/view/render.rs` already documents view types that exceeded recursive trait limits and produced multi-megabyte symbols that the macOS linker rejected.
Its existing `.boxed()` boundaries are important.
The application is about 50,500 lines of Rust, including tests, and compiles as one binary target.
The font library is a separate target but shares the package's dependency and feature configuration.
Moving functions between files within a target does not create a new compilation boundary.

An isolated experiment changed `recipes::inspector_group` to return `Box<AnyWidgetView<Workspace>>` and boxed its result.
After cleaning only the temporary Runebender package's artifacts, its binary unit took 64.19 seconds, compared with 75.07 seconds in the clean baseline.
Dependencies were cached for the experiment, so its overall build time is not a clean-build comparison.
The unit's high memory reading remained essentially unchanged.
This is an exploratory result, not a proven memory fix or a ready-to-merge patch.
The experiment did not receive the Gray/Light, browser, input, or runtime-performance validation needed for promotion.
Profile the remaining generated types and distinguish rustc code generation from linking before choosing further boundaries.

## What Babelfont adds

The root manifest enables Babelfont's `types`, `glyphs`, and `fontir` features with defaults disabled.
It also directly depends on `fontc` and `fontdrasil` 1.0.0.
The resolved `fontc` feature set is empty: its CLI and optional Rayon defaults are already disabled.
Simply adding more `default-features = false` declarations is not the missing optimization.

| Baseline unit | Time |
|---|---:|
| Babelfont library | 3.54 s |
| Babelfont build-script compilation | 0.27 s |
| Babelfont build-script execution | 0.79 s |
| `fontir` 1.0.0 | 3.26 s |
| `fontbe` 1.0.0 | 6.68 s |
| `fontc` library | 1.00 s |

Babelfont's library invocation had a recorded maximum RSS of approximately 0.63 GiB.
The full compiler stack has additional dependencies; the table is not its total cost.

The target-filtered graph contains 419 reachable normal/build packages, including Runebender, excluding root development dependencies.
Counterfactual graph walks give the following attribution:

| Root dependency edges excluded | Packages no longer reachable | Summed baseline unit durations for those exact package versions |
|---|---:|---:|
| Babelfont only | 8 | 12.00 s |
| `fontc` only | 0 | 0 s |
| Babelfont and `fontc` | 38 | 50.92 s |
| Babelfont, `fontc`, and `fontdrasil` | 41 | 70.45 s |

These are dependency-graph counterfactuals, not working application builds with those features removed.
The sums overlap in real time and cannot be subtracted from the 114.4-second wall time.
They also omit replacement-code cost, changes in feature unification, application monomorphization, and downstream linking effects.
Of 429.95 summed unit-seconds, Babelfont-exclusive packages account for about 2.8%, or about 11.8% when its shared compiler route and the direct `fontc` edge are both excluded.
Those percentages describe summed compiler-unit elapsed durations, not CPU utilization or percentages of user waiting time.
Cargo explains unit timing and pipelining in its [timings documentation](https://doc.rust-lang.org/cargo/reference/timings.html).

The eight Babelfont-exclusive packages are Babelfont, `csv`, `csv-core`, `fea-rs-ast`, `skrifa` 0.46.2, `sr-aef`, `typeshare`, and `typeshare-annotation`.
Other expensive packages remain reachable through the application and compiler.

Babelfont is also embedded in Runebender's own geometry ownership, persistence adapters, compiler snapshots, and variable-font model.
Its compile burden is therefore not exhausted by timing the external crate.
Quantifying that indirect cost requires a functionally equivalent alternative implementation; source size alone cannot establish it.

## Other findings and priorities

1. **Fix build-script invalidation first.**
   It is the smallest demonstrated improvement to the edit/build loop.
   Add explicit input tracking while preserving the Windows stack setting.

2. **Investigate the application compilation before undertaking a backend rewrite for speed.**
   Preserve existing type-erasure boundaries, then use compiler profiling and linker measurements to select further boundaries.
   Consider concrete view types or boxed boundaries at substantial panels, with runtime and accessibility validation.
   Do not indiscriminately box every widget or split every module into a crate.

3. **Offer a contributor profile with less debug information.**
   Apply it consistently to development and tests, with a documented full-debug alternative.
   This has measured disk benefits but is not sufficient to solve the largest memory peak.
   Keep incremental compilation enabled for normal local development.

4. **Consolidate font-library versions deliberately.**
   The native graph has two `fea-rs` versions (0.22 and 1.0), two `write-fonts` versions (0.44 and 0.52), two `fontdrasil` versions (0.4 and 1.0), three `skrifa` versions (0.40, 0.44, 0.46), and four `read-fonts` versions (0.36, 0.37, 0.41, 0.43).
   Runebender's direct shaping dependencies use the older feature compiler and writer while `fontc` uses the newer versions.
   A coordinated update of `src/text/shape.rs` and binary import is worth investigating.
   Some older versions remain required by Xilem, Parley, Vello, or Babelfont's feature AST; changing direct dependency versions cannot remove every duplicate.
   Font shaping, generated tables, variable behavior, and browser builds require validation after such upgrades.

5. **Separate compiler and importer costs from the data model.**
   `--no-default-features --lib` removes application dependencies from a build, but still includes Babelfont's compiler route, direct `fontc`, importers, and image tracing.
   `fontc` 1.0.0 unconditionally depends on UFO, Glyphs, and Fontra frontends despite Runebender supplying an in-memory IR source.
   An upstream feature split for those frontends is a better first investigation than removing the working font compiler.
   Runebender's direct `glyphslib` importer already handles Glyphs import; Babelfont's `glyphs` feature is a candidate for removal, but was not tested here.
   Babelfont imports and applies `typeshare` annotations unconditionally in its source, so removing `types` is not a safe manifest-only assumption at this pin.

6. **Give the font engine a truly independent package boundary if profiling justifies it.**
   The current library and binary are separate targets in one package.
   Default application dependencies are consequently also attached to library builds; anonymous imports in `src/lib.rs` explicitly accommodate this.
   A cohesive engine/application split would allow stable core features and engine-only tests without GUI dependencies.
   `masonry_testing` is currently an unconditional package dev-dependency, so engine-only test workflows also need attention.
   Keep compiler, tracing, and source import adapters separable without inventing fallback behavior that silently changes editor functionality.

7. **Keep full release settings out of the routine contributor loop.**
   Native and browser release profiles use thin LTO and one codegen unit.
   Native release uses the default optimization level 3; the browser uses level 2.
   An opt-in fast optimized profile with LTO off, more codegen units, and incremental compilation is worth benchmarking for interactive editor work.
   No release-profile speedup was measured here; do not change release quality based on these debug results.

8. **Make validation and cache usage predictable.**
   There are 20 integration-test targets and three examples in addition to library and binary tests.
   Full tests, all-target Clippy, docs, release builds, and the separate browser workspace legitimately create distinct artifacts.
   Prefer the relevant check or test during iteration and run the complete prescribed checks at promotion boundaries.
   Preserve the CI checks; improve the workflow rather than removing coverage.
   The native cache was 26 GiB before this review, including 12 GiB of incremental files and 11 GiB of debug dependencies; the browser cache was another 6.2 GiB.
   Avoid repeated fresh target directories and unnecessary `RUSTFLAGS`, profile, feature, or toolchain changes during ordinary development.
   The browser preparation script also rewrites its generated adapter manifest each run; avoid identical rewrites if fingerprint inspection confirms they cause work.

## Implications for a Babelfont alternative

A replacement may be justified by model quality, exact precision, preservation, or architectural ownership.
This review does not support prioritizing it as the principal compile-time remedy.
Keep the model limited to font data, geometry, stable source/layer identities, and editing semantics.
Make serialization formats and the `fontc` adapter separate dependencies or feature-controlled layers.
Avoid making a geometry-only consumer compile feature AST rewriting, format importers, TypeScript annotations, or a font compiler.
Retain the current source/layer, metadata, fractional-value, round-trip, undo, interpolation, and compilation contracts when evaluating an alternative.
Moving Runebender's approximately 5,900-line Babelfont adapter into a new dependency without simplifying those responsibilities would not by itself establish a build improvement.

## Lower-memory contributor target

Treat 8 GiB and 16 GiB systems as explicit test configurations.
Apple specifies 8 GB for [MacBook Neo](https://support.apple.com/en-nz/126322); Framework Laptop 12 configurations and generations differ, so record the actual processor, RAM, and operating system used in each test rather than relying on the product name.
Start with the repository's existing two-job guidance on hosts below 24 GiB, keep Cargo commands sequential, and reduce to one job if necessary.
A job limit restricts concurrency; it cannot guarantee that one large compiler or linker invocation fits in memory.

Suggested acceptance goals, not measured guarantees:

- An 8 GiB machine can build while keeping an editor open, without sustained swap pressure or an out-of-memory failure.
- A representative UI edit rebuilds in under 10 seconds on the actual target laptop.
- Cold compilation fits an agreed several-minute budget, measured separately from downloading dependencies.
- Memory, target-directory size, UI edits, engine edits, focused tests, and full tests are recorded independently.
- The editor remains responsive in the contributor profile, with Gray/Light screenshots and native interaction checks preserved.

This review did not benchmark a Framework Laptop 12, MacBook Neo, Intel Mac, Linux, Windows, WebAssembly, release builds, or the complete test suite.
It is a build-focused architecture and dependency review, not a full functional audit of the application.

## Reproduction and evidence

The local evidence directory is `/tmp/runebender-build-review`.
It contains clean Cargo timing reports, dependency metadata, per-invocation measurements, logs, and the temporary source copy.
Temporary compiled target directories were cleaned after evidence was preserved; the contributor's native and browser caches were left intact.
The original instrumented trial failed to preserve Cargo jobserver file descriptors and was excluded from the reported timing comparisons.
The corrected wrapper uses `subprocess.run(..., close_fds=False)` and forwards compiler diagnostics.
Per-invocation resource figures use macOS `getrusage(RUSAGE_CHILDREN)` and must not be interpreted as the sum of simultaneously resident processes.

Commands for comparable clean builds, using separate empty target directories:

```sh
CARGO_TARGET_DIR=/tmp/runebender-baseline cargo build --locked --offline --timings
CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_TARGET_DIR=/tmp/runebender-lines cargo build --locked --offline --timings
```

Use these commands sequentially and add `CARGO_BUILD_JOBS=2` on lower-memory hosts as required by `AGENTS.md`.
Do not clean the contributor's working target directory merely to reproduce a cold benchmark.

## Implementation follow-up

The build script now tracks only `build.rs`, preserving the existing Windows stack reservation.
Its MSVC, GNU, and macOS output branches were exercised directly.

Query-level profiling identified `dimensions_section` in `src/application/view/panels/editor_info.rs` as the dominant type-checking query: 36.66 seconds in the detailed trace.
Explicitly fixing its row state with `xrow::<Workspace, (), _>` avoids inferring it through the iterator, collection, and enclosing section.
The same annotation addresses the smaller sidebar total-row hotspot.
Neither change adds a box, changes a widget type, or changes runtime behavior.
The broad inspector-boxing experiment was reverted because it did not reduce the expensive inference phase.

With a fresh incremental directory for the application binary and the same pinned compiler, the Dimensions annotation alone changed these compiler-phase results:

| Measurement | Before | After Dimensions annotation |
|---|---:|---:|
| Type checking | 40.869 s | 4.676 s |
| RSS at end of type checking, compiler-reported decimal MB | 7,545 MB | 889 MB |
| Total application compiler invocation | 77.081 s | 37.778 s |

These are compiler-invocation measurements with dependencies available, not full clean-build wall times.
Diagnostic flags were used only for profiling; no nightly feature, bootstrap environment setting, or custom compiler flag is required by the changes.

Direct dependencies now reuse `fea-rs` 1.0.0, `write-fonts` 0.52, and `skrifa` 0.46.2, versions already present through the existing compiler/model dependencies.
The shaping adapter uses the new fallible `GlyphMap::new` and propagates its error.
No new dependency packages were added.
The native lockfile lost eight packages and the browser lockfile lost seventeen.
Native `fea-rs`, `write-fonts`, and `fontdrasil` now each have one version; `skrifa` has two and `read-fonts` has three because the remaining versions are required upstream.

Implementation evidence and diagnostic logs are under `/tmp/runebender-build-improvements`.

Validation also exposed two stale checks already present in the baseline.
The menu-order test still expected the labels replaced by commit `343e04e`; its two expected labels now match that committed behavior.
The Norad boundary allowlist still used seven paths moved by commit `81abd5d`; those entries now name the same codec boundaries in their current locations.
The gate additionally verifies that every allowlisted file exists, so a later move produces a direct diagnostic.
No architecture check was removed or skipped.

The suite plus the corrected boundary gate's focused rerun passed 954 tests; four existing ignored tests remain excluded from runtime coverage.
Strict all-target Clippy, documentation generation, formatting, copyright headers, and advisory checks passed.
Gray and Light editor PNGs were byte-identical before and after the changes.
Expanded Dimensions panels were also rendered and inspected in both themes with Virtua Grotesk.

### Final native build comparison

An archived copy of the same baseline commit, overlaid with the final source and lockfile changes, was built with an empty target directory and the same default development profile, pinned compiler, twelve Cargo jobs, offline sources, and resource wrapper.
No reduced-debug profile or special compiler flags were used.

| Measurement | Baseline | Final changes |
|---|---:|---:|
| Clean native build wall time | 114.4 s | 75.1 s |
| Application compiler invocation, including link | 75.07 s | 36.73 s |
| Application invocation resource high-water mark | 10.31 GiB | 2.66 GiB |
| Font-engine library invocation | 9.25 s | 9.16 s |
| Small UI label edit and rebuild | 7.07 s | 2.49 s |

Clean wall time improved by approximately 34%, and the application invocation's memory high-water mark decreased by approximately 74%.
The small UI edit produced only an application binary compiler invocation; the font-engine library remained cached.
The final clean build compiled 495 Cargo units, compared with 507 in the baseline.
Babelfont's library invocation took 2.99 seconds with a 0.68 GiB resource high-water mark in the final build.
Its contribution still does not justify replacing the font model for build performance alone.

These remain single-run measurements on the 48 GiB M4 Pro, not measurements on an 8 GiB laptop.
The per-invocation memory figure is not the peak memory of all concurrent compiler processes.
The existing two-job guidance for hosts below 24 GiB remains appropriate; an actual low-memory machine trial remains useful before promising a specific build time there.
The optimized native release build also passed with the existing LTO and optimization settings.
Final timing reports and per-invocation records are retained under `/tmp/runebender-build-improvements`.
Temporary profiling caches and the clean benchmark's compiled target directory were removed after retaining the evidence; the checkout's normal native and browser caches were preserved.

The browser release build and strict WebAssembly Clippy check passed with the updated browser lockfile.
The existing `web/quality.cjs` suite passed in headless Chrome at device pixel ratios 1, 2, and 1.25, including its input, undo/redo, export, theme, scaling, and idle assertions.
Browser Gray and Light editor captures were inspected as well.
This completes the three prioritized fixes: build invalidation, application compiler-memory use, and dependency consolidation.
Remaining upstream duplicate versions and unmeasured low-memory hardware are follow-up opportunities, not evidence that Babelfont remains the dominant bottleneck.
