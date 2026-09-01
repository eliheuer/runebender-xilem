# AGENTS.md

Context for anyone, human or agent, working on `runebender-core`.
The reference for how the code is organized is
[runebender.org/docs/code-layout.html](https://runebender.org/docs/code-layout.html).
This file is the short version plus what you need to build and
submit a change.

## What this is

The font library behind the Runebender font editor, with no
interface. One rule decides what belongs here: if an operation
changes a font, or reads one to answer a question, it lives in this
crate. The front-ends own the window, the input, and the drawing.
The `runebender-core` binary in `src/bin` exposes the same operations on
the command line.

The in-memory font is `norad::Font`. Functions take norad types or
kurbo geometry and return the same. There is no private model.

## Layout

`src/` is six directories, one per concern. Each `mod.rs` says what
belongs in it. Read those six comments first.

| Directory | Holds |
|---|---|
| `outline/` | what changes a shape, and `path/` for the segment maths |
| `analysis/` | what reads a font |
| `formats/` | lib keys, and every format besides UFO |
| `document/` | `Master`, `Project`, interpolation, composites, `model/` |
| `text/` | `shape` (harfrust), `joining` (Arabic rules), `buffer` (the Text tool) |
| `ui/` | themes, sidebar data, `editing/` (selection, undo, viewport) |
| `bin/runebender.rs` | the command line |

Paths follow the tree: `runebender_core::outline::glyph_ops`. There
are no re-exports at the root except three types.

## Features

One feature, `cli`, on by default, carrying the command line and its
only dependency, clap. The editors depend on this crate with
`default-features = false`, so nothing that opens a window builds
clap. Keep it that way: a dependency that only the binary needs goes
behind this feature.

## Build and test

```sh
cargo build
cargo test        # about 300 tests
cargo fmt
cargo clippy --all-targets
```

Tests that need a real font load Virtua Grotesk from
`../virtua-grotesk/sources`, or from `$RUNEBENDER_TEST_FONTS`.
`src/testing/fonts.rs` is the one place that knows. Clone
[virtua-grotesk](https://github.com/eliheuer/virtua-grotesk) beside
this repository or set the variable.

## The gate

CI runs on every push, on Linux and macOS, and at the minimum Rust in
`Cargo.toml`: `cargo fmt --check`, `cargo clippy --all-targets`,
`cargo doc --no-deps`, and `cargo test`, with warnings denied. The
manifest forbids `unsafe` and warns on `missing_docs`, so a public
item without a doc comment fails the build.

CI's stable can be newer than yours. If clippy passes locally and
fails there, run it under the toolchain CI reports.

## Conventions

- One file, one concern, named for what it does to a font. Its header
  comment says so in a sentence.
- Every public item has a doc comment that says what it does, what it
  returns, and any precondition or side effect. Wrap type names in
  backticks.
- An operation that edits in place returns whether it changed
  anything, or how many things it changed.
- A UFO lib key has one constant, one reader, and one writer, in
  `formats/lib_keys.rs` or the module for its format.
- Tests live next to the code, in a `tests` module at the bottom of
  the file.
- No path to a sibling checkout in a committed file. Local overrides
  go in a `.cargo/config.toml` above the repositories.
- Edition 2024. Line width 100.

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

- Commit locally as you work. Push when a phase is coherent. Squash
  iteration commits before pushing; never squash pushed commits.
- Commit messages say why. The diff shows what.
- No `Co-Authored-By` trailers for agents.
- Stage explicit paths. Never `git add -A`; checkouts carry
  uncommitted work.

## Consumers

Both editors pin this crate by git revision in their `Cargo.toml`.
After pushing a change here, bump both pins.
