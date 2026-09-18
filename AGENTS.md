# AGENTS.md

Runebender's native editor, headless tools, and reusable font engine live in the root `runebender` Cargo package.
The browser host is a separate Cargo workspace under `web/` that reuses the root sources through a small compatibility package.
Sibling repositories are not workspace members; do not modify them unless a task explicitly spans repositories.

## Architecture

Read `ARCHITECTURE.md` before moving code or adding a subsystem.
It is the canonical source map and includes a change-routing guide for humans and agents.

The root package produces the `runebender` executable and its library target.
A font path starts the Xilem editor; a subcommand runs headlessly before window setup.
Modules under `src/analysis`, `src/document`, `src/formats`, `src/outline`, `src/text`, and `src/ui` contain reusable font and toolkit-independent editor behavior.

The in-memory font is `norad::Font`.
Keep application state and platform work out of the font-engine modules.
Keep font mutations, analysis, formats, shaping, interpolation, selection, and undo in those modules when they can be shared.

| Path | Responsibility |
|---|---|
| `src/lib.rs` | font-engine module root |
| `src/main.rs` | executable composition root |
| `src/application/` | all Xilem application and runtime code |
| `src/application/cli.rs` | arguments and headless adapters |
| `src/application/editor/` | editing commands, sessions, and inspectors |
| `src/application/editor/tools/` | named editor tools and tool-like workflows |
| `src/application/platform/` | files, watching, live endpoints, and screenshots |
| `src/application/view/` | application views and canvas widgets |
| `src/application/widgets/` | reusable widgets missing from the framework |
| `src/{analysis,document,formats,outline,text,ui}/` | font engine by concern |
| `web/` | separate browser workspace for the shared widget tree |

Read the module header for the area you change.
Keep one concern per file where that remains clearer than another layer of indirection.

## Build and test

The root toolchain is pinned in `rust-toolchain.toml`, and primary Linux/macOS CI must use the same version with warnings denied.
The Windows workflow is a second-class diagnostic on current stable: keep its failures visible and document support limits, but do not add platform-specific complexity unless the task is about Windows.

```sh
cargo fmt --all --check
bash .github/copyright.sh
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
cargo test --workspace --locked -- --test-threads=1
cargo build --workspace --release --locked
cargo deny --locked check advisories
```

Set `RUNEBENDER_TEST_FONTS` to a directory containing test UFOs and a designspace when the adjacent Virtua Grotesk checkout is unavailable.
Do not count ignored model tests as passing runtime coverage.

For a clean-checkout proof, clone or archive the repository into a temporary directory and run the documented commands there.
Do not add local path patches to a committed Cargo configuration.

## Interface work

Read `DESIGN.md` before changing a view.
Use `view::theme` for colors, `view::design` for measurements, and `view::recipes` for repeated controls.
Views read workspace state; commands own intent; the font engine owns font behavior.

Use the headless screenshot path for visual checks.
Inspect Gray and Light when UI code changes, and wait for idle auto-hide before accepting a capture.
A headless image does not prove native pointer, IME, accessibility, or GPU behavior.
Prefer headless validation; launch a foreground GUI only when the task requires interactive evidence and the user has agreed to the interruption.

The browser reuses the desktop widget tree but has an in-memory font and a separate Cargo workspace.
When shared sources change, run its build and smoke checks as described in `web/README.md`.

## Platform boundaries

- macOS uses `muda` for the operating-system menu bar.
- Linux, Windows, and the browser use the in-window Masonry menu.
- Live editor sockets are Unix-only; keep headless file commands portable.
- File dialogs are an application concern.
  Font-engine modules must not depend on them.

Do not turn a failing platform check green by removing it.
Record an honest support limit in `docs/known-limitations.md` when a failure cannot be reproduced or fixed in scope.

## Rust conventions

- Follow the current Linebender canonical lint and rustfmt sets.
- Every public library item needs a useful doc comment.
- An in-place edit returns whether or how much it changed.
- A UFO lib key has one constant, reader, and writer.
- Tests live beside the code they verify.
- Edition 2024; line width 100; no `unsafe` in workspace code.
- Prefer a reasoned `expect` or a structural fix to a local lint allowance.

## Documentation conventions

- Follow the Linebender formatting scheme: put each prose sentence on its own source line.
- Update `ARCHITECTURE.md`, `DESIGN.md`, or `docs/known-limitations.md` when a change invalidates their guidance.

## Dependency policy

Commit `Cargo.lock` and use `--locked` in CI.
Git dependencies must name an exact revision, not a branch.
CI runs `cargo deny --locked check advisories`; every ignored advisory needs a comment explaining the affected dependency and removal condition.
Review dependency additions and upgrades explicitly rather than treating generated policy files as evidence that their code was audited.

## Changes and Git

Record user-visible changes under `Unreleased` in `CHANGELOG.md`.
Preserve unrelated working-tree changes.
Stage explicit paths, never `git add -A`.
Commit coherent phases with messages that explain why, and do not add agent co-author trailers.
Do not force-push or remove other worktrees.
