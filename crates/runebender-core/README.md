# Runebender Core

The library behind the [Runebender](https://github.com/eliheuer/runebender-xilem)
editor and its headless commands. It contains font loading, editing, analysis,
formats, interpolation, shaping, selection, and undo, with no GUI dependency.

Core is an internal workspace package rather than a separate executable. Use
the root `runebender` command:

```sh
runebender info Font.ufo --json
runebender proof Font.ufo --glyphs H,n,o --out proof.svg
runebender mcp --font Family.designspace
```

The [AI-assisted type-design notes](docs/ai-type-design.md) describe the local
proposal and review model. Some command examples there predate workspace
consolidation; use `runebender --help` as the authoritative command surface.

## License

Apache-2.0 OR MIT
