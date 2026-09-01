# Changelog

All notable changes to runebender-core. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

No releases yet. The first release is planned and `RELEASING.md`
describes how it will be cut. Until then, `main` is the only line
and this section stays open.

### Removed

- `info`. It counted glyphs, layers and codepoints, which is not a
  question worth a command.

- `measure` and `check`. `measure` reported what the editor shows and
  fontTools prints in a line. `check` compared point signatures across
  two masters, which `fonttools varLib.interpolatable` does more
  thoroughly across a whole designspace.
- `color` and `spacing`, with `analysis::optical` and
  `analysis::spacing`. Both produced lists a designer had to ignore:
  `color` flagged glyphs drawn denser on purpose, and `spacing`
  reported every sidebearing against a grid it inferred from a font
  that has none.

### Changed

- The command line is behind a `cli` feature, on by default. Editors
  depending on this crate with `default-features = false` no longer
  build clap.

