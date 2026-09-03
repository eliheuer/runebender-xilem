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
MCP client such as Claude Code; nothing in it edits the font.

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
