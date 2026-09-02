# Changelog

All notable changes to runebender-xilem. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

No releases yet. `AGENTS.md` has the checklist for the first one.
Until then, `main` is the only line and this section stays open.

### Changed

- Undo lives in core. `Session` holds an `EditHistory` from
  `runebender_core::document::history` instead of its own `UndoState`,
  and records, undoes, and discards through it.
