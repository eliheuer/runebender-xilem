# Known limitations

Runebender is experimental. This file states what the automated evidence does
and does not establish as of 2026-09-15.

## Native platforms

Linux and macOS compile, lint, document, and test in CI. Most interactive work
has been exercised on macOS. Native Linux pointer behavior, input methods, file
dialogs, accessibility, and GPU/driver combinations still need hands-on tests.

Windows builds successfully, and its headless `info`, SVG proof, and CPU editor
render checks pass. At commit `c42baf8`, the native startup smoke test created a
window and then exited with access violation `0xC0000005` (`-1073741819`). The
same signature remained after isolating macOS menu handling at `e43700b` and
after a remote trial without source-tree watcher polling at `9c342f0`. Neither
bounded hypothesis fixed it, so the watcher remains available and the cause is
still unknown. The workflow remains failing until a subsequent Windows run
verifies a fix. See
[the Windows workflow](https://github.com/eliheuer/runebender-xilem/actions/workflows/windows.yml)
and `scripts/windows-smoke.ps1`.

[Issue #13](https://github.com/eliheuer/runebender-xilem/issues/13) reports a
crash on an Apple-silicon Mac running a beta macOS release. It includes a user
log but no reduced reproducer or triaged cause, so passing current macOS CI does
not close it.

The live editor endpoint and Local Chat bridge use Unix sockets and are not
available on Windows. Headless file commands remain portable.

## Input and local models

Unit tests cover text-buffer bidi behavior, shaping, selection, replacement,
and deletion. Browser tests dispatch paste and composition events. These checks
do not prove native candidate-window placement or complete IME behavior on each
desktop platform.

The normal suite ignores four tests that need the adjacent full Virtua Grotesk
sources or installed local models. Process contracts and proposal safety are
tested without a model, but an actual local-chat conversation and model-backed
proposal remain supervised validation tasks.

## Browser demo

The browser build reuses the Rust Xilem/Masonry widget tree and edits a bundled
font in memory. Refreshing resets the document. User-file import/export,
persistent saves, local subprocesses, native accessibility forwarding, and node
execution are desktop-only. The automated Chromium matrix does not certify
Safari, Firefox, every operating-system IME, or native GPU rendering. See
`web/README.md` and `web/VALIDATION.md`.

## Periodic lint review

The ordinary canonical Linebender lint set is enforced with warnings denied.
The broader periodic pass also reports advisory findings, chiefly unrelated
binding shadowing, mixed public/private fields, and large enum variants. Those
are not automatic edits: changing them would churn test narratives, public API
shape, or the action representation without fixing a demonstrated behavior.

This cleanup applied the unambiguous subset: a single-use lifetime was elided,
equivalent match arms were merged where the grouping stayed clear, a returned
`Self` gained `must_use`, and a dead application wrapper was removed. Run the
periodic command from the Linebender canonical-lints page before a release and
review new findings individually.

## Supply-chain coverage

`cargo deny --locked check advisories` passes. Refreshing the configured public
cargo-vet imports added real coverage for `block2` 0.6.2. Eight exact versions
remain unvetted: `rfd` 0.17.2 and the pinned Xilem revision's `masonry`,
`masonry_core`, `masonry_testing`, `masonry_winit`, `tree_arena`, `xilem`, and
`xilem_core`. The repository does not claim those packages were audited, and no
new exemption is added merely to make CI green.

## Reproducible evidence

- `docs/parity/2026-09-11/REAL-WORK-TRIAL.md` records a disposable-font editing,
  proposal, install, undo, save, and reopen workflow.
- `docs/parity/2026-09-11/V04-LONG-INPUT-REPRO.md` records the compact text-input
  clipping reproducer retained for regression work.
- `docs/browser-quality/2026-09-14/README.md` records the browser interaction and
  density matrix.
- `docs/visual-audit/2026-09-14/` retains matched Gray and Light screenshots for
  recent interface changes. These images are visual evidence, not platform or
  input certification.
