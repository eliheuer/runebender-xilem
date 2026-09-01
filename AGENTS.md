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
lives here in application code, and `docs/XILEM-GAPS.md` records it.

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

## Porting between the two editors

The same editor is built twice, on GPUI and on Xilem. A feature that
lands in one should be cheap to carry to the other, so the two share
a file layout: the same concern lives at the same path in both, and
a change is a diff you can read side by side.

Mirror where the concern is shared. Diverge where the framework
forces it, and say so in the file's own module comment. Do not force
a match that costs either editor clarity.

| Concern | Both |
|---|---|
| `main()` and the module list | `main.rs` |
| The `Workspace` struct | `workspace.rs` |
| Actions, the menu bar, the keymap | `actions.rs` |
| The event loop and the first frame | `launch.rs` |
| What the window shows | `view/` |
| What the user does | `edit/` |
| The world outside the window | `platform/` |
| Toolkit pieces the framework lacks | `widgets/` |
| The glyph canvas and the grid | `view/canvas/` |
| One file per panel region | `view/panels/` |
| Files, and reloading one master | `platform/host.rs` |
| Watching for other writers | `platform/watch.rs` |

Where they differ on purpose:

| GPUI | Xilem | Why |
|---|---|---|
| `wiring.rs` | `view/recipes.rs` | GPUI builds input widgets once and subscribes. Xilem rebuilds views from state every frame, so there is nothing to wire; what repeats becomes a recipe. |
| GPUI's own scale | `view/design.rs` | GPUI ships `px_1`, `text_xs`, `rounded_md`. Xilem takes a number wherever a measurement goes, so the scale is application code. |
| `view/blur.rs` | none | GPUI blurs box shadows and nothing else, so the preview's blur is rasterized on the CPU. Vello blurs what it is asked to. |
| `RB_OPEN_GLYPH` | `--bin screenshot` | Two ways to see a frame without clicking. Xilem has a headless render path; GPUI opens on a named glyph instead. |
| `widgets/` | `widgets/` | Same directory, different contents: each toolkit is missing different things. |

When one editor gets ahead, the port is: read the file at the same
path in the repository that has the feature, and write the same
decomposition here. If it needs a new file, give it the name the
other one uses, so the next port in the other direction is a diff.

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

## The interface

`DESIGN.md` says how to change what a person looks at: the token
rule, the canvas and the chrome, how interface text is worded, and
the mistakes worth knowing by name. Read it before touching a view.
The tokens themselves are `view/theme.rs` and `view/design.rs`.

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
audits and exemptions, and CI runs `cargo vet --locked`. CI also runs
`cargo deny check advisories`, which is the other half: vet says
where a crate came from, deny says whether anyone has published a
vulnerability against it. `deny.toml` holds the ignore list, one
entry per advisory with the reason and what would let it go. When you
add or bump a dependency, run `cargo vet` and record the result on
purpose. Releases do not exist yet; `RELEASING.md` is the checklist
for the first one, and user-visible changes go under `Unreleased`
in `CHANGELOG.md`.

## Git

- Commit locally as you work. Push when a phase is coherent.
- Commit messages say why. The diff shows what.
- No `Co-Authored-By` trailers for agents.
- Stage explicit paths. Never `git add -A`.
