# Runebender Core

[![CI](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml)

This is the core of the [Runebender](https://runebender.org) font
editing system. It exists without a front-end so that we can have
multiple GUIs, primarily
[Runebender-GPUI](https://github.com/eliheuer/runebender-gpui) and
[Runebender-Xilem](https://github.com/eliheuer/runebender-xilem).
Runebender-Core can also be used as a headless CLI tool by agents or
in bash scripts.
`runebender-core mcp --font <designspace>` serves the same tools to an
MCP client. It can inspect glyphs and create proposals; it cannot install them.
`runebender-core compose <ufo> --write` derives precomposed glyphs from
their base and marks through anchors, as a proposal.

`runebender-core features <ufo> --write` writes the `mark` and `mkmk`
features the font's anchors imply to `features.generated.fea` in the
UFO and includes it from `features.fea`, so a compiled font positions
marks the way the editor's preview does. Without `--write` it prints
them.

## AI-assisted type design

[Architecture, research, and the working tool contract](docs/ai-type-design.md)
explain the Counterpunch/Blender comparison and the local AI roadmap.
Agents can select a master, read glyph revisions, submit exact batched edits with
`propose_edits`, and proof the resulting layer. Python is an optional client;
[the spacing example](examples/propose_spacing.py) uses only the CLI and standard library.

## Use

As a library, in `Cargo.toml`:
```toml
runebender-core = { git = "https://github.com/eliheuer/runebender-core" }
```

As a command line tool:
```sh
cargo install --git https://github.com/eliheuer/runebender-core
```

## License

Apache-2.0 OR MIT
