# Runtime performance pass, 25 September 2026

This pass follows the build improvements committed as `11223a6`.
It focuses on repeated work in the native and browser application without adding dependencies, background services, or new cache invalidation machinery.

## Changes

- The default empty-search, All-category, name-ordered glyph view shares its existing immutable cell list.
  Previously every call visited every glyph, performed search preparation and cloned every retained cell into a new allocation.
  The default path is now constant-time and preserves pointer identity, allowing the grid's existing rebuild check to skip a full cell-order comparison.
- Other searches reject excluded categories immediately, skip string matching for empty queries, and stop after a successful name match.
  Unicode query normalization now happens once per search rather than once per encoded glyph.
- Sidebar category and language counts borrow a zero-or-one-codepoint slice from an `Option` instead of allocating a vector for each encoded glyph.
- Character-target coverage scans the targets directly instead of allocating and populating a hash set for every glyph being tested.
  Name matches, multiple codepoints, empty codepoint lists, and range fallback retain their existing semantics.

These changes use existing types and dependencies and do not change the concrete Xilem view tree.
They deliberately avoid persisting filtered results that would need new invalidation rules for source edits, search modes, categories, sort order, or palette changes.

## Other paths reviewed

The font model already shares outline paths with `Arc` and exposes targeted entry refreshes.
The grid already paints visible cells only and preserves scrolling when a rebuilt filtered list retains its order.
Native preview compilation already coalesces pending requests and caches by document revision and compiler structure.
The browser still compiles previews synchronously; moving that work off-thread would require a separate architecture change.

Two drawing paths remain candidates for profiling: grid row packing is recomputed for paint and hit testing, and canvas labels are shaped for measurement and drawing.
Caching those results may help larger fonts, but introduces additional invalidation or memory-budget rules.
Neither is claimed to be a measured bottleneck in this pass.

## Validation

The full native suite passed 956 tests, with four existing ignored tests excluded from runtime coverage.
The new regressions exercise search scope, case sensitivity, Unicode prefixes, regular expressions, category exclusion, alternate codepoints, and coverage fallbacks.
Strict native Clippy, documentation generation, formatting, and copyright checks passed.
Gray and Light headless editor captures were inspected and were byte-identical to the build-improvement baseline.

The earlier build-improvement commit is pushed to `origin/main`; this runtime pass is a separate change set.

## Measurements

A release-mode microbenchmark calls the actual library function against the four built-in target-bearing filters, using 2,000 synthetic glyphs with codepoints 0 through 1,999, repeated 100 times.
This produces 800,000 predicate calls per trial; both versions return exactly 18,300 matches.
The baseline and changed binaries were measured alternately in three trials after validation builds finished, on the same M4 Pro used for the build review.

| Trial | Before | After |
|---|---:|---:|
| 1 | 334.94 ms | 82.18 ms |
| 2 | 342.14 ms | 61.41 ms |
| 3 | 335.39 ms | 61.26 ms |

The median improved from 335.39 ms to 61.41 ms, approximately 5.5 times faster for this operation.
This is a synthetic coverage-predicate benchmark, not an overall frame-rate or startup-speed claim.
The unfiltered cell-list improvement is established by its constant-time shared-list path and a regression assertion that it preserves pointer identity; no whole-application speedup is inferred from that alone.
Benchmark sources, binaries, compiler metrics, and validation logs are in `/tmp/runebender-runtime-review`.

A development application build with dependencies available and a fresh application incremental directory took 36.71 seconds for the compiler invocation, with a 2.66 GiB resource high-water mark.
The preceding build-improvement baseline was 36.73 seconds and 2.66 GiB.
Both used the pinned compiler, full development debug information, and the same resource wrapper; the new run used no diagnostic compiler flags.
These single-run results show no meaningful application compile-time or memory regression; they are not a new full clean-build comparison.
Native and browser optimized release builds and strict browser Clippy also passed.
The full browser interaction suite passed at 1×, 2×, and 1.25× display scale, including search input, selection, undo/redo, font export, and idle rendering assertions.
Browser Gray and Light captures at 1× were byte-identical to the preceding baseline.
The final offline advisory check passed against the cached advisory database, and neither dependency lockfile changed.
