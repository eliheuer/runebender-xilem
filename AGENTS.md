# AGENTS.md

Context for anyone, human or agent, working on `runebender-xilem`.
The reference for how the code is organized is
[runebender.org/docs/code-layout.html](https://runebender.org/docs/code-layout.html).
This file is the short version plus what you need to build and
submit a change.

## What this is

The Runebender font editor's front-end on Xilem, the Linebender UI
stack. It is the other half of a controlled comparison with
[runebender-gpui](https://github.com/eliheuer/runebender-gpui): the
same editor, the same core, built twice, so that what differs is the
framework. It owns the window, the input, and the drawing, and calls
[runebender-core](https://github.com/eliheuer/runebender-core) for
everything that changes a font or reads one. If a change you are
making does font work with no Xilem in it, it belongs in core.

It builds against upstream Xilem pinned to a revision, not a fork.
Where Xilem has no answer for something the editor needs, the answer
lives here in application code, and `XILEM-GAPS.md` records it.

## Layout

The layout mirrors runebender-gpui file for file where the two
share a concern, so a change in one editor is easy to carry to the
other. Each directory has a `mod.rs` that says what belongs in it.

| Path | Holds |
|---|---|
| `main.rs` | `main()` and the module list |
| `workspace.rs` | the `Workspace` struct and the types it is made of |
| `actions.rs` | the action list, the native menu bar, and the keymap behind both |
| `launch.rs` | the event loop, the window, and the first frame |
| `model.rs` | the font model: a loaded font plus a per-glyph cache. The next thing to replace with core's `Master` and `Project` |
| `view/` | what the window shows: `canvas/` (editor, grid), `panels/` (one file per region), `chrome`, `render`, `design`, `recipes`, `theme` |
| `edit/` | what the user does: `commands`, `inspector`, `session`, `sidebar`, `text_tool` |
| `platform/` | the world outside the window: `host` (files), `watch`, `screenshot` |
| `widgets/` | icon tiles, text labels, the context menu, the shortcut host |

Where the two editors differ: GPUI builds its input widgets in
`wiring.rs`; Xilem's views are rebuilt every frame from state, so
there is no wiring step and `recipes.rs` holds the view functions
that repeat. `design.rs` is the design system Xilem does not ship.

## Build and test

```sh
cargo run path/to/Font.designspace
cargo test
cargo fmt
cargo clippy --all-targets
```

`rust-toolchain.toml` pins stable. To work on core at the same time,
clone it beside this repository and put a `paths` override in a
`.cargo/config.toml` above both checkouts, never inside either.

`cargo run --bin screenshot` renders one frame to a PNG with no
window, which is how the interface is checked without launching it.
Do not launch the GUI to check your work while the user is at the
machine.

## The gate

CI runs on every push, on Linux and macOS: `cargo fmt --check`,
`cargo clippy --all-targets`, `cargo doc --no-deps`, `cargo test`,
and a release build, with warnings denied. The Linux job installs
the libraries winit and Vello link against.

CI's stable can be newer than yours. If clippy passes locally and
fails there, run it under the toolchain CI reports.

## Conventions

- Call `theme::` accessors instead of naming a colour, and the
  `design::` tokens instead of a size, radius, or stroke width.
- A command is the whole of one intent, in `edit/commands.rs`.
- Views read the workspace; they do not hold state.
- No path to a sibling checkout in a committed file.
- Core is pinned by git revision in `Cargo.toml`. Bump it when core
  changes.

## Supply chain and releases

Dependencies are vetted with cargo-vet; `supply-chain/` holds the
audits and exemptions, and CI runs `cargo vet --locked`. When you
add or bump a dependency, run `cargo vet` and record the result on
purpose. Releases do not exist yet; `RELEASING.md` is the checklist
for the first one, and user-visible changes go under `Unreleased`
in `CHANGELOG.md`.

## Git

- Commit locally as you work. Push when a phase is coherent.
- Commit messages say why. The diff shows what.
- No `Co-Authored-By` trailers for agents.
- Stage explicit paths. Never `git add -A`.
