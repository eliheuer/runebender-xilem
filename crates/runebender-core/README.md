# Runebender Core

[![CI](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml)

The shared font library in the [Runebender workspace](https://github.com/eliheuer/runebender-xilem).
The application calls it from both the editor and headless subcommands.
Core has no executable or GUI dependency.

Run `runebender info Font.ufo --json` to inspect a font, or
`runebender mcp --live` to expose the live editor tools to an MCP client.

## AI-assisted type design

[Architecture, research, and the working tool contract](docs/ai-type-design.md)
explain the Counterpunch/Blender comparison and the local AI roadmap.
Agents can select a master, read glyph revisions, submit exact batched edits with
`propose_edits`, and proof the resulting layer. Python is an optional client;
[the spacing example](examples/propose_spacing.py) uses only the CLI and standard library.

## Use

Install the application from the workspace:

```sh
cargo install --git https://github.com/eliheuer/runebender-xilem
runebender --help
```

## License

Apache-2.0 OR MIT
