# AGENTS.md

This directory is Runebender's internal font library. The repository-root
`AGENTS.md` defines workspace, Git, validation, and supply-chain rules; this
file adds Core-specific boundaries.

If an operation reads or changes a font, it belongs here. The application owns
windows, input, drawing, dialogs, subprocesses, and command-line parsing. Core
is library-only and must not gain a second executable.

The in-memory font is `norad::Font`. APIs accept norad types or Kurbo geometry
and return the same rather than introducing a second private font model.

| Directory | Responsibility |
|---|---|
| `outline/` | shape changes and segment mathematics |
| `analysis/` | measurements and other read-only font queries |
| `formats/` | lib keys and non-UFO formats |
| `document/` | masters, projects, interpolation, composites, and model data |
| `text/` | shaping, joining, bidi layout, and the Text tool buffer |
| `ui/` | shared themes, sidebar data, selection, undo, and viewport state |

Public items need doc comments that explain results, preconditions, and side
effects. In-place edits return whether or how much they changed. A UFO lib key
has one constant, one reader, and one writer. Put tests at the bottom of the
file they cover.

Run Core checks from the repository root so the application and library are
validated together:

```sh
cargo test --workspace --locked -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
```
