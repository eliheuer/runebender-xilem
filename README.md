# Runebender Core

[![CI](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml)

This is the core of the [Runebender](https://runebender.org) font
editing system. It exists without a front-end so that we can have
multiple GUIs, primarily
[Runebender-GPUI](https://github.com/eliheuer/runebender-gpui) and
[Runebender-Xilem](https://github.com/eliheuer/runebender-xilem).
Runebender-Core can also be used as a headless CLI tool by agents or
in bash scripts.

## Use

As a library, in `Cargo.toml`:

```toml
runebender-core = { git = "https://github.com/eliheuer/runebender-core" }
```

As a command, for the edits worth doing to many files at once:

```sh
cargo install --git https://github.com/eliheuer/runebender-core

runebender-core clean Family.designspace
runebender-core overlap Font.ufo --glyphs cent,euro
runebender-core kern Font.ufo --left A --right V --set -80
```

Each takes any number of UFOs or designspaces, so one line reaches a
whole family.

`--dry-run` writes nothing and exits 1 when there is work waiting.
`--json` is on every command. `runebender-core --help` lists the
rest.

## License
Apache-2.0 OR MIT
