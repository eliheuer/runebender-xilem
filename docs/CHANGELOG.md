# Changelog

Notable user-facing changes will be recorded here and copied into each GitHub Release's notes.
Entries under Unreleased describe work being prepared, not an available public release.

## Unreleased

### Added

- Local macOS application and drag-to-Applications DMG packaging for maintainer testing, with a temporary application icon.
- Live MCP tool `analyze_font`: one audit of a whole source, with counts by mark label, off-grid points, kinks, stem medians per case and missing or incompatible glyphs against every other source, worst offenders named.
- Live MCP tool `analyze_glyph`: stem and bar, ink spans, node continuity, grid fit, mark label, and point-structure compatibility with every other source, with a reason.
- The Chat panel serves a selected GGUF through a supervised `llama-server` child when that binary is found beside the editor, at `RUNEBENDER_LLAMA_SERVER`, or on PATH: loopback only, the model's own chat template for tool calls, thinking off by default, one server kept across turns and stopped with the panel. `font-ml chat` remains the fallback. A chat model folder now needs only a `.gguf`.
- A `triage-font` workflow, served as an MCP prompt and a skill, that runs `analyze_font` and ranks what needs the designer's eyes.
- The editor removes dead live-session sockets left by crashed or killed editors when it starts.
- `analyze_glyph` on the saved-file tool surface too (`runebender agent call analyze_glyph --font`), with compatibility against the other masters of a designspace.
- Chat panel tool set now includes `analyze_glyph`, `analyze_font`, `glyph_inventory`, `editor_context`, `read_kerning` and `specimen`, and the prompt tells the model to quote measured numbers rather than compute them from points. Typing `/review-glyph H` or another workflow name sends that workflow's text.
- `runebender mcp --install` writes the live server, by the binary's absolute path, into Claude Code, Codex, Claude Desktop and Cursor settings; `--dry-run` reports without writing.
- A `runebender` hub skill and prompt with the connection, targeting and safety rules; every skill under `skills/` declares `surface: runebender-live-v1` and the host refuses another surface.
- Live MCP workflows served as prompts (`review-glyph`, `compatibility-check`, `spacing-pass`, `kerning-pass`, `draft-weight`); the same files under `skills/` can be linked into a client's skills folder.

### Changed

- The canonical source repository is now [fontgarden/runebender](https://github.com/fontgarden/runebender).
  Existing GitHub repository links redirect to the new location.

### Known limitations

- The first public release is still in preparation; the local DMG is ad-hoc signed, and Developer ID distribution has not yet been validated.
- Windows desktop installation is not a supported release target.

Before v0.1 is published, curate the editor's initial feature list, tested platforms and source formats, installation requirements, and remaining limitations here.
See [RELEASING.md](RELEASING.md) for the release workflow and publication gates.
