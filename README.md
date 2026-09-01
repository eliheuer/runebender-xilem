# Runebender-Xilem

[![CI](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-xilem/actions/workflows/ci.yml)

The [Runebender](https://runebender.org) font editor built on
[Xilem](https://github.com/linebender/xilem), against upstream rather
than a fork. It shares
[runebender-core](https://github.com/eliheuer/runebender-core) with
[Runebender-GPUI](https://github.com/eliheuer/runebender-gpui), the
main editor: the same editor built twice, so that what differs is the
framework. This one is behind the GPUI build.

## Use

```sh
cargo install --git https://github.com/eliheuer/runebender-xilem
runebender-xilem path/to/Font.designspace
```

The manual is at [runebender.org](https://runebender.org/docs/).

## License

Apache-2.0 OR MIT
