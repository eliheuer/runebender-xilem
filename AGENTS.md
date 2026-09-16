# AGENTS.md

Runebender is one application and one Cargo package. Keep all changes in this
repository; sibling repositories are not workspace members.

## Architecture

The package produces the `runebender` executable and its internal library target.
A font path starts the Xilem editor; a subcommand runs headlessly before window
setup. Modules under `src/analysis`, `src/document`, `src/formats`, `src/outline`,
and `src/text` contain operations that read or change a font.

The in-memory font is `norad::Font`. Keep application state and platform work
out of the font-engine modules. Keep font mutations, analysis, formats, shaping,
interpolation, selection, and undo in those modules when they can be shared.

| Path | Responsibility |
|---|---|
| `src/lib.rs` | font-engine module root |
| `src/main.rs` | executable composition root |
| `src/cli.rs` | arguments and headless adapters |
| `src/launch.rs` | native event loop and window setup |
| `src/workspace.rs` | application and open-document state |
| `src/actions.rs` | one action table for menus and shortcuts |
| `src/edit/` | application editing commands and session adapters |
| `src/platform/` | files, watching, live endpoints, and screenshots |
| `src/view/` | application views and canvas widgets |
| `src/widgets/` | reusable widgets missing from the framework |
| `src/{analysis,document,formats,outline,text,ui}/` | font engine by concern |
| `web/` | browser host for the shared widget tree |

Read the module header for the area you change. Keep one concern per file where
that remains clearer than another layer of indirection.

## Build and test

The Rust toolchain is pinned. CI runs Linux and macOS with warnings denied, plus
a separate Windows smoke workflow.

```sh
cargo fmt --all --check
taplo fmt --check --diff
bash .github/copyright.sh
typos
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
cargo test --workspace --locked -- --test-threads=1
cargo build --workspace --release --locked
```

Set `RUNEBENDER_TEST_FONTS` to a directory containing test UFOs and a
designspace when the adjacent Virtua Grotesk checkout is unavailable. Do not
count ignored model tests as passing runtime coverage.

For a clean-checkout proof, clone or archive the repository into a temporary
directory and run the documented commands there. Do not add local path patches
to a committed Cargo configuration.

## Interface work

Read `DESIGN.md` before changing a view. Use `view::theme` for colors,
`view::design` for measurements, and `view::recipes` for repeated controls.
Views read workspace state; commands own intent; the font engine owns font behavior.

Use the headless screenshot path for visual checks. Inspect Gray and Light when
UI code changes, and wait for idle auto-hide before accepting a capture. A
headless image does not prove native pointer, IME, accessibility, or GPU
behavior. Do not open a foreground GUI while the user is using the machine.

The browser reuses the desktop widget tree but has an in-memory font and a
separate Cargo workspace. When shared sources change, run its build and smoke
checks as described in `web/README.md`.

## Platform boundaries

- macOS uses `muda` for the operating-system menu bar.
- Linux, Windows, and the browser use the in-window Masonry menu.
- Live editor sockets are Unix-only; keep headless file commands portable.
- File dialogs are an application concern. Font-engine modules must not depend on them.

Do not turn a failing platform check green by removing it. Record an honest
support limit in `docs/known-limitations.md` when a failure cannot be reproduced
or fixed in scope.

## Rust conventions

- Follow the current Linebender canonical lint and rustfmt sets.
- Every public library item needs a useful doc comment.
- An in-place edit returns whether or how much it changed.
- A UFO lib key has one constant, reader, and writer.
- Tests live beside the code they verify.
- Edition 2024; line width 100; no `unsafe` in workspace code.
- Prefer a reasoned `expect` or a structural fix to a local lint allowance.

## Supply chain

Dependencies are checked with `cargo vet --locked` and
`cargo deny --locked check advisories`. Imports, audits, and exemptions live in
`supply-chain/`. Never add an audit without reviewing the required code and
criteria. Never add or broaden an exemption silently. Dependency changes must
include an explicit provenance decision.

## Changes and Git

Record user-visible changes under `Unreleased` in `CHANGELOG.md`. Preserve
unrelated working-tree changes. Stage explicit paths, never `git add -A`.
Commit coherent phases with messages that explain why, and do not add agent
co-author trailers. Do not force-push or remove other worktrees.
