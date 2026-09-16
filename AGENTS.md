# AGENTS.md

Runebender is one application and one Cargo package. Keep all changes in this
repository; sibling repositories are not workspace members.

## Architecture

Read `ARCHITECTURE.md` before moving code or adding a subsystem. It is the
canonical source map and includes a change-routing guide for humans and agents.

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
| `src/app/` | all Xilem application and runtime code |
| `src/app/cli.rs` | arguments and headless adapters |
| `src/app/editor/` | editing commands, sessions, and inspectors |
| `src/app/editor/tools/` | named editor tools and tool-like workflows |
| `src/app/platform/` | files, watching, live endpoints, and screenshots |
| `src/app/view/` | application views and canvas widgets |
| `src/app/widgets/` | reusable widgets missing from the framework |
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

## Dependency policy

Commit `Cargo.lock` and use `--locked` in CI. Git dependencies must name an exact
revision, not a branch. CI runs `cargo deny --locked check advisories`; every ignored
advisory needs a comment explaining the affected dependency and removal condition.
Review dependency additions and upgrades explicitly rather than treating generated
policy files as evidence that their code was audited.

## Changes and Git

Record user-visible changes under `Unreleased` in `CHANGELOG.md`. Preserve
unrelated working-tree changes. Stage explicit paths, never `git add -A`.
Commit coherent phases with messages that explain why, and do not add agent
co-author trailers. Do not force-push or remove other worktrees.
