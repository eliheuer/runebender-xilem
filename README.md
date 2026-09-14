# Runebender

[![CI](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml)

A Linebender-native font editor built on [Xilem](https://github.com/linebender/xilem).
This repository contains the application and its independent font library in
one Cargo workspace.

The package and executable are named `runebender`. Start it without arguments
to open the editor, or use a subcommand to work without a window.

## Workspace

- Root package: `runebender`, the editor and headless command line.
- `crates/runebender-core`: the shared font library, with no GUI dependency.
- `cargo run -- --help`: discover commands.
- `cargo test --workspace -- --test-threads=1`: test both packages.

Core tests use `RUNEBENDER_TEST_FONTS`, or the `virtua-grotesk/sources`
directory beside this repository. New font-library work belongs in this
workspace, not the legacy standalone Core repository.

## Use

```sh
cargo install --git https://github.com/eliheuer/runebender-xilem
runebender path/to/Font.designspace
runebender info path/to/Font-Regular.ufo --json
runebender mcp --live
```

The user manual and documentation is available at
[runebender.org](https://runebender.org/docs/).

## License

Apache-2.0 OR MIT
