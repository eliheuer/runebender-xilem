# Changelog

All notable changes to runebender-core. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

No releases yet. The first release is planned and `RELEASING.md`
describes how it will be cut. Until then, `main` is the only line
and this section stays open.

### Added

- Editing commands in the `runebender-core` binary: `clean`, `overlap`,
  `offset`, `convert`, `realign`, `rename`, and `unicode`.
  They take any number of UFOs or designspaces, a `--glyphs` filter,
  and `--dry-run`, so a font library can be swept from a shell script
  or by an agent.
- `runebender-core glyphs`, which lists glyph names one per line for
  piping into `xargs`.

### Removed

- `measure` and `check`. `measure` reported what the editor shows and
  fontTools prints in a line. `check` compared point signatures
  across two masters, which `fonttools varLib.interpolatable` does
  more thoroughly across a whole designspace.
- `kern`. Writing a pair created a glyph-to-glyph exception over the
  group kerning the value came from, and wrote an explicit zero where
  the editor removes the pair.
- `color` and `spacing`, with `analysis::optical` and
  `analysis::spacing`. Both produced lists a designer had to ignore:
  `color` flagged glyphs drawn denser on purpose, and `spacing`
  reported every sidebearing against a grid it inferred from a font
  that has none.

### Changed

- The command line is behind a `cli` feature, on by default. Editors
  depending on this crate with `default-features = false` no longer
  build clap.

- The binary is a directory, `src/bin/runebender/`, split into
  reading commands, editing commands, and source expansion.
