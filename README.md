# Runebender Xilem

[![CI](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml)

A Linebender-native font editor built on [Xilem](https://github.com/linebender/xilem).
This repository contains the application and its independent font library in
one Cargo workspace. GPUI is retained separately as a reference and fallback.

## Workspace

- Root package: the Xilem editor. `cargo run --release -- <font>` opens it.
- `crates/runebender-core`: font operations and a headless CLI, with no GUI dependency.
- `cargo run -p runebender-core -- --help`: discover headless commands.
- `cargo test --workspace -- --test-threads=1`: test both packages.

Core tests use `RUNEBENDER_TEST_FONTS`, or the `virtua-grotesk/sources`
directory beside this repository. New font-library work belongs in this
workspace, not the legacy standalone Core repository.

## Use

```sh
cargo install --git https://github.com/eliheuer/runebender-xilem
runebender-xilem path/to/Font.designspace
```

The user manual and documentation is available at
[runebender.org](https://runebender.org/docs/).

## License

Apache-2.0 OR MIT
