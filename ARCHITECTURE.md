# Runebender architecture

Runebender is one Cargo package with one executable. The package has two targets:

- `src/lib.rs` exposes the font engine used by the editor and headless commands.
- `src/main.rs` is the composition root for the Xilem application and command line.

The library target is an internal boundary, not a second product or frontend. It keeps font
behavior testable without constructing a window and lets the Xilem browser build reuse the same
engine.

## Design goals

1. **The filesystem is the first index.** A contributor should be able to guess where a feature
   lives before using search.
2. **Use font-editor language.** Prefer names such as `glyph`, `outline`, `metaballs`, `text`, and
   `workspace` over generic names such as `manager`, `service`, or `utils`.
3. **Separate behavior from presentation.** Font operations do not depend on Xilem. Views show
   state; editor modules interpret user intent; the font engine performs reusable operations.
4. **Keep roots small.** `src/` contains `main.rs`, `lib.rs`, the `application/` boundary, and the font
   engine's domain modules. Application implementation files do not accumulate beside the roots.
5. **Teach through module headers.** Every module root says what belongs there and, when useful,
   points to the next layer involved in the same feature.

## Source map

```text
src/
├── lib.rs                 font-engine public map
├── main.rs                executable composition root
├── analysis/              read and measure font data
├── document/              projects, masters, history, workflows
├── formats/               source formats and persistent metadata
├── outline/               reusable geometry and outline operations
├── text/                  shaping, joining, features, text layout
├── ui/                    toolkit-independent editor data
└── application/           Xilem application
    ├── mod.rs             application map
    ├── actions.rs         shared menu and shortcut action table
    ├── cli.rs             headless command adapter
    ├── font_model.rs      application-facing project cache
    ├── workspace.rs       open-document and presentation state
    ├── launch.rs          native startup and headless capture
    ├── browser.rs         browser host for the same Xilem view tree
    ├── editor/            user intent and editing interaction
    │   ├── commands.rs
    │   ├── session.rs
    │   ├── inspector.rs
    │   ├── sidebar.rs
    │   └── tools/         named tools and tool-like workflows
    ├── platform/          files, dialogs, watching, live IPC, screenshots
    ├── view/              Xilem views and Masonry canvas widgets
    └── widgets/           reusable UI primitives missing from Xilem/Masonry
```

The desired dependency direction is:

```text
main.rs → application → runebender library
             ↘ Xilem / Masonry / platform adapters

view → workspace + editor
editor → workspace + font engine
workspace → font_model + font engine
font engine -X→ Xilem, Masonry, dialogs, or window state
```

`main.rs` only selects the command-line or graphical launch path. Application modules import
dependencies from their real locations under `application/`; there is no crate-root prelude or
second set of module names to keep in sync.

## Where do I make a change?

| Goal | Start here | Related engine code |
|---|---|---|
| Add or change an editor tool | `application/editor/tools/` | usually `outline/` or `text/` |
| Change Metaballs | `application/editor/tools/metaballs.rs` | `outline/metaballs.rs`, `formats/metaballs.rs` |
| Change text-mode interaction | `application/editor/tools/text.rs` | `text/buffer/`, `text/shape.rs` |
| Change Nodes interaction | `application/editor/tools/nodes.rs` | `document/nodes*.rs`, `ui/nodes.rs` |
| Change selection or undo | `application/editor/session.rs` | `ui/editing/`, `document/history.rs` |
| Add a menu item or shortcut | `application/actions.rs` | `application/editor/commands.rs` |
| Change the edit canvas | `application/view/canvas/editor.rs` | `application/editor/session.rs` |
| Change a panel | `application/view/panels/` | matching editor or document module |
| Change reusable control styling | `application/view/recipes.rs` | `application/view/design.rs`, `theme.rs` |
| Add a file format | `formats/` | dispatch in `document/project.rs` |
| Change variable-font ownership or source edits | `document/project.rs`, `document/variable.rs` | `document/source.rs` compatibility projections |
| Change axis conversion or interpolation | `document/axis.rs`, `document/var_model.rs`, `document/interpolation.rs` | `formats/designspace.rs` |
| Add a headless command | `application/cli.rs` | operation in the matching library domain |
| Change native or browser hosting | `application/platform/`, `application/launch.rs`, `application/browser.rs` | none |

For a first reading, follow this path:

1. `src/main.rs` — decide between a headless command and the editor.
2. `src/application/mod.rs` — see the application pieces.
3. `src/application/workspace.rs` and `font_model.rs` — understand application state.
4. `src/application/editor/session.rs` — understand one active glyph-editing session.
5. `src/application/view/render.rs` — see how the application modes compose their views.
6. `src/lib.rs` — enter the reusable font domains as needed.

## Variable font document

`document::project::Project` owns a variable font, even when opened from a single UFO.
`document::variable` stores one glyph with all of its source, intermediate and auxiliary layers, addressed by `SourceId` and `LayerId`.
`Project::glyph_sources` identifies the subset participating in that glyph's interpolation; an auxiliary layer does not become a source merely by existing.
Babelfont owns geometry; the preserving adapter retains exact UFO values and metadata that Babelfont cannot represent.
Stable source identities survive insertion, removal and display-order changes.
New read-only callers use `document_glyph`, `document_layer`, `document_source` and `document_sources` to inspect canonical geometry, exact metrics and stable identities without constructing UFO values.
New mutations use `edit_document_layer` and its owned `LayerEditDraft`; a failed or unchanged draft is discarded, while a committed draft advances the canonical revision once.
`document/sources.rs` owns structural transactions and their guarded undo history; removing a source never deletes its UFO directory.

`document::source::Master` is a compatibility UFO projection with source-local history and paint caches.
Project exposes immutable projections through `sources()` and scoped mutations through `edit_source`, `edit_sources`, and `active_font_mut`.
Dropping an edit guard reconciles additions, removals, geometry and metadata into canonical glyph storage before the next Project operation.
Use `edit_layer` and `undo_layer` for a specific glyph layer without switching the active editor source.
Default-layer edits share the existing editor history, while auxiliary layers have independent histories.
Do not introduce another mutable source-font accessor.

Project save materializes UFOs from canonical Babelfont geometry and preserving Norad templates, preserving font info, libs, layer order, features, kerning, groups, images and data.
The templates and projections currently duplicate some data to preserve compatibility with existing Norad algorithms.
They are not separate editable documents.
Native reload, live edits, proposals, experiments and browser edits cross the same scoped mutation boundary.

`document::axis` wraps pinned Babelfont coordinate conversion without exposing its types.
`document::var_model` wraps the fontdrasil variation backend used by Babelfont, retaining f64 values with rounding disabled.
`document::interpolation` checks structure and interpolates advances, contours, anchors and component transforms using each glyph's own sources.
Component outlines resolve recursively at the same location, with explicit failures for missing or cyclic components.
The application reads these results instead of maintaining a second interpolation implementation.

`document::compile` builds a complete Babelfont snapshot and compiles it with fontc in Rust.
The same immutable OpenType bytes feed HarfRust shaping, Skrifa variable outlines and TTF export.
`document::compile_metadata` supplies UFO metadata and Designspace rules to that compiler.
The native preview worker coalesces pending edits and publishes only the current revision; slider changes reuse the compiled font.
The browser currently compiles synchronously and downloads exported bytes through a thin platform binding.
Application source controls live in `application/editor/sources.rs`; views dispatch commands and never perform font mutations themselves.

The [dependency and format decision](docs/variable-project-decision.md) records the upstream precision blocker, exact references, preservation policy and supported boundaries.
The [migration checklist](docs/babelfont-migration-checklist.md) tracks removal of the remaining Norad editing model while retaining source-format preservation.

## Adding a tool

1. Put interaction state and pointer/key behavior in `application/editor/tools/<tool>.rs`.
2. Put reusable geometry or font mutations in the matching library domain, most often `outline/`.
3. Put canvas painting in `application/view/canvas/` and controls in `application/view/panels/`.
4. Add its command and shortcut to the shared action table rather than creating a second dispatch
   path.
5. Test the font operation without a window; test interaction with the smallest useful Masonry
   harness.
6. Add the tool to the routing table above when its location would not be obvious to a newcomer.

## File and module size

Line count is a prompt to inspect responsibility, not a target to game. At roughly 500 lines,
check whether a file now contains independently nameable concerns. Above 1,000 lines, prefer a
small module directory with a clear `mod.rs` map when a real boundary exists. Do not split a
cohesive algorithm merely to satisfy a number.

Current large files are architectural debt, not examples to copy. Good future splits include CLI
subcommands by domain, editor-canvas input/paint/layout, and project loading/saving/history. Make
those splits when working in the area so behavior changes and movement can be reviewed together.

## Reference projects

This layout borrows principles rather than copying another editor's technology choices:

- [Linebender Xilem](https://github.com/linebender/xilem/blob/main/ARCHITECTURE.md) keeps crate
  roots small and groups code by durable roles such as widgets, properties, passes, and views.
- [Fontra](https://github.com/fontra/fontra) makes editor, font overview, font info, core, and
  storage backends visible in its directory structure.
- [Shift](https://github.com/shift-editor/shift) documents explicit application/domain boundaries, named
  tool and command locations, and a file-size review guideline. Runebender adopts the navigation
  discipline without adopting Shift's multi-package architecture.
- [Counterpunch](https://github.com/counterpunchspace/editor) maintains prominent architecture and
  developer-documentation areas beside its application. Runebender keeps one canonical guide here
  rather than scattering per-directory context files.

GPUI Runebender remains a behavioral and visual reference. It is not the architecture template;
new application structure should stay idiomatic to Xilem and Masonry.
