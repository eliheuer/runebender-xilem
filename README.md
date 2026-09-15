# Runebender

[![CI](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml)
[![Windows basics](https://github.com/eliheuer/runebender-xilem/actions/workflows/windows.yml/badge.svg)](https://github.com/eliheuer/runebender-xilem/actions/workflows/windows.yml)

Runebender is an experimental font editor built in Rust with
[Xilem](https://github.com/linebender/xilem). This repository is the complete
Cargo workspace: the graphical editor and headless commands are one
`runebender` executable, backed by the internal `crates/runebender-core`
library.

Runebender is suitable for testing and development, not production font work
without backups. Platform and interaction limits are tracked in
[Known limitations](docs/known-limitations.md).

## Install and run

Install Rust 1.96 or newer, then build from source:

```sh
git clone https://github.com/eliheuer/runebender-xilem.git
cd runebender-xilem
cargo run --release -- path/to/Font.designspace
```

The editor accepts UFOs and designspaces. With no path it opens a file picker.
Basic, import-only Babelfont packages are also supported; unsupported data is
rejected rather than silently discarded.

Install the executable directly from Git:

```sh
cargo install --git https://github.com/eliheuer/runebender-xilem --locked
runebender path/to/Font-Regular.ufo
```

Linux builds need the Wayland, X11, Vulkan, OpenSSL, GLib, GTK, and xdo
development libraries listed in [.github/workflows/ci.yml](.github/workflows/ci.yml).
Windows builds use the 64-bit MSVC Rust toolchain and its Visual Studio C++
prerequisites.

## Headless tools

Subcommands finish before any window is created:

```sh
runebender --help
runebender info path/to/Font.designspace --json
runebender proof path/to/Font.ufo --glyphs H,n,o --out proof.svg
runebender mcp --font path/to/Font.designspace
```

`agent`, `compose`, `features`, `nodes`, `proposal`, and `propose` expose the
same font operations used by the editor. Commands that propose changes keep
them separate for review and explicit installation.

## Workspace

- `src/`: application, command line, platform adapters, views, and widgets.
- `crates/runebender-core/`: font data, editing, formats, shaping, and analysis;
  it has no GUI dependency and no separate executable.
- `web/`: the same Xilem/Masonry widget tree compiled to a self-contained WASM
  demo. It uses an in-memory bundled font and does not save user files.
- `docs/`: current limitations plus dated reproducible evidence from earlier
  implementation and browser-quality passes.

The user documentation is at [runebender.org](https://runebender.org/docs/).

## Develop

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
cargo test --workspace --locked -- --test-threads=1
cargo build --workspace --release --locked
cargo vet --locked
cargo deny --locked check advisories
```

Tests that need a full font read `RUNEBENDER_TEST_FONTS`, or
`../virtua-grotesk/sources` when that repository is beside this one. Four
model- or fixture-dependent tests are ignored by default and are not part of
the ordinary test count.

The canonical development rules are in [AGENTS.md](AGENTS.md); visual changes
also follow [DESIGN.md](DESIGN.md).

## License

Apache-2.0 OR MIT
