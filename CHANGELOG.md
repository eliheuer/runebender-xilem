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
  `offset`, `convert`, `realign`, `rename`, `unicode`, and `kern`.
  They take any number of UFOs or designspaces, a `--glyphs` filter,
  and `--dry-run`, so a font library can be swept from a shell script
  or by an agent.
- `runebender-core glyphs`, which lists glyph names one per line for
  piping into `xargs`.

### Changed

- The binary is a directory, `src/bin/runebender/`, split into
  reading commands, editing commands, and source expansion.
