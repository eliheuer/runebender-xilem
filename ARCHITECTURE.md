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
`outline::glyph_paths::ordinary_layer_contours_to_bezpath` converts canonical contour views directly for geometry consumers that do not need components or hyperbezier solving.
`outline::glyph_paths::ordinary_layer_to_bezpath` preserves canonical contour/component order, applies exact component transforms and reports missing references or cycles through a caller-supplied layer resolver.
`outline::segment_ops` enumerates and hit-tests ordinary canonical segments with stable source identities, including the control pairs that define implied quadratic endpoints.
`analysis::measure` accepts canonical ordinary layers directly for live measurements and side-bearing geometry while compatibility callers finish migrating.
`analysis::curve::ordinary_cubics_from_layer` supplies canonical ordinary contours to continuity and curvature analysis without a UFO glyph.
New mutations use `edit_document_layer` and its owned `LayerEditDraft`; a failed or unchanged draft is discarded, while a committed draft advances the canonical revision once.
Semantic glyph-mark edits set or clear the label and typed public color together; the theme maps its palette label to that typed color before opening the document transaction.
Layer drafts address points by stable `PointId` for individual movement, snapped handle-aware dragging, smooth-state changes and selection transforms without positional remapping.
Persistent point drags capture stable origins for selected points, carried handles and smooth-coupled handles before the first snapped event.
They also shift contour points and anchors together for left-sidebearing edits while exact advance changes remain explicit metric operations.
Direct line-to-cubic conversion inserts newly identified canonical controls and sets every accepted geometric-line endpoint to cubic while retaining endpoint identity and metadata, including on wraparound closing segments.
Direct topology operations create pen, rectangle and ellipse contours with stable identities before any UFO projection is refreshed.
Trace and SVG format boundaries append or replace only their explicit contour payloads through validated layer drafts, assigning fresh document identities without reconciling a whole glyph.
Imported contours are validated as a complete serializable glyph candidate before commit, including topology and identifier uniqueness, and replacement retains the existing contour/component paint slots when their counts permit it.
The hyperbezier pen creates, appends and closes typed canonical hyper contours directly, retaining stable on-curve identities and a fresh UFO compatibility marker.
Direct segment subdivision supports stored-endpoint lines, quadratics and cubics, preserving existing control identities and metadata while assigning fresh identities to inserted topology and rejecting nonfinite computed geometry before mutation.
Quadratic subdivision represents stored and implied endpoints explicitly; it validates that implied pairs still belong to a quadratic chain and materializes a midpoint before moving either defining control.
Direct point deletion rebuilds only affected canonical contours from surviving objects, retaining their identities and source metadata while removing dependent incoming controls; deleting a quadratic control materializes its implied endpoints and replaces only that segment with a line.
Point deletion stages the complete operation so an error in a later contour cannot commit changes to an earlier contour.
Direct contour reversal reorders canonical points and transfers incoming segment roles without replacing objects; closed contours retain their first stored point so reversal is an exact involution, and symmetric storage reports no change.
Changing a closed contour's start rotates canonical nodes and preservation records together, retaining point and contour identities.
Opening a contour removes the chosen endpoint's incoming controls, rotates the surviving on-curve point to a canonical move point and retains the surviving objects; closing changes the existing move to a line.
Contour copy carries canonical geometry and source metadata without a UFO projection; paste and duplicate assign fresh document identities plus fresh UFO identifiers for copied objects that carry identifiers or libraries.
Boolean and overlap operations consume canonical paths and replace the affected topology with fresh contour and point identities, empty source metadata and restored smooth flags at retained on-curve positions; a successful empty result removes the contour block while components retain their identities and relative order.
Explicit mask baking decodes the UFO mask key at its boundary, subtracts marked canonical contours and clears the key only after successful replacement.
Knife preview and slicing consume canonical cubic, quadratic, mixed-degree, all-off-curve and hyperbezier contours directly; missed contours retain their identities and exact metadata, while sliced contours receive fresh identities and empty source metadata.
Quadratic control runs are normalized to explicit implied joins only inside the path-engine adapter, leaving canonical source topology untouched.
Cleanup, coordinate rounding, path-direction correction, cubic-handle fitting and extrema insertion mutate canonical contours while retaining every surviving object's identity and source metadata.
Learned and model-predicted embolden operations move canonical points in place, preserving topology, identities and exact source metadata.
Component decomposition resolves nested canonical layer shapes, rounds transformed output at the existing command boundary, preserves source names and libraries and assigns fresh identities to the pasted contours.
Editable hyperbezier kind is an explicit contour-preservation field rather than an inference from the current UFO identifier; copy, duplicate and decomposition assign fresh hyper-marked UFO identifiers without changing the contour kind.
Explicit hyperbezier conversion solves selected canonical contours directly and replaces only that topology with fresh cubic identities and empty source metadata.
Explicit metaball collapse samples and fits selected or all live groups on a staged canonical layer draft, retaining the editable source data until every replacement contour succeeds.
Stroke expansion, offset, extrusion and roughening consume canonical paths directly; replaced topology receives fresh identities and empty source metadata while untargeted contours, components and anchors retain their exact objects.
The committed `DocumentChange` identifies direct and component-dependent layers and whether geometry, metrics, metadata or compilation became stale.
Source-wide feature text, groups and exact fractional kerning have canonical ownership in `VariableData` by stable `SourceId` and change through `edit_document_source_metadata`; UFO templates no longer retain second editable values for them.
Source image-resource insertion uses a stable-`SourceId` Project operation that validates UFO image rules and updates persistence state without exposing a mutable source font.
`document_snapshot` clones Babelfont glyph geometry, exact extensions, typed source metadata and stable source order without cloning UFO templates or Master projections.
Interpolation compatibility diagnostics compare canonical contour and point topology under stable source identities rather than reading Master projections.
`document_source_glyph_entries` derives sorted grid names, Unicode, advances, semantic marks and paint paths directly from canonical default layers; unresolved components retain their intrinsic contours in the grid.
`CanonicalLayerSnapshot` captures one opaque addressed layer with the same geometry and extensions; guarded restore compares the complete live state before replacing it, advances the revision once and refreshes the compatibility projection without recording legacy history.
Multi-source glyph metadata uses staged layer drafts and one batch publication; Unicode replacement validates the complete source set before mutation, advances the revision once and refreshes every changed projection.
`CanonicalSourceMetadataSnapshot` captures feature text, groups and exact kerning for the complete stable source set; guarded whole-snapshot restore ignores display reorder, rejects stale or changed source sets and refreshes all affected projections in one revision.
Auxiliary-layer copy and removal mutate canonical Babelfont layers and exact extensions first, then refresh only the affected compatibility projection.
Background send, swap and clear stage a complete canonical source snapshot and record guarded source-history transactions for both standalone UFO and Designspace documents.
Send copies contours and exact width into the conventional background while omitting unrelated glyph metadata; swap exchanges contours, retains the foreground width and writes that width to the background, matching the editor command's established behavior.
Review proposals use those auxiliary layers under stable source identities; revision-checked batches stage canonical drafts before publication, and guarded installation records Project-owned foreground history.
The external edit-batch drawing schema remains a UFO-shaped wire contract, while `SetOutline` decodes that payload directly into canonical contours without projecting or reconciling a whole glyph.
Experimental versions clone canonical layer drafts and canonical source metadata for the session, and they apply selected changes only after root-baseline conflict checks.
The GLIF SHA and external UFO proposal format remain explicit transient codec boundaries rather than editable Norad mirrors.
`formats::ufo` provides read-only detached UFO values for format adapters and fixtures; there is no corresponding whole-glyph reconciliation path into Project.
`document/sources.rs` owns structural transactions and their guarded undo history; removing a source never deletes its UFO directory.
`document/filesystem.rs` loads complete UFO and Designspace source sets before construction and stages every save artifact before replacing live destinations.
The native file watcher resolves nested feature includes through Project and fingerprints those dependencies with the UFO and Designspace roots so a changed external include blocks overwrite.
Headless source information, SVG proof and proposal commands open one explicit Project source, read canonical layers and metadata, and save proposal mutations through Project persistence.
Transient experiment proof and Designbot adapters accept detached source-font values directly and do not construct an editable Master wrapper.

`document::source::Master` is a compatibility UFO projection with source-local history and transitional paint caches.
Project exposes immutable projections through `sources()` and scoped mutations through `edit_source`, `edit_sources`, and `active_font_mut`.
Dropping an edit guard reconciles additions, removals, geometry and metadata into canonical glyph storage before the next Project operation.
Use `edit_layer` and `undo_layer` for a specific glyph layer without switching the active editor source.
Default-layer edits share the existing editor history, while auxiliary layers have independent histories.
Do not introduce another mutable source-font accessor.
The production application no longer reads this projection or its history; `FontModel::master`, `font`, `master_mut` and `font_mut` are test-only retirement fixtures.
Application undo ordering and stale-task checks use stable canonical layer addresses, Project-owned history depths and canonical glyph revisions.

Project save materializes UFOs from canonical Babelfont geometry and glyph-free source-format data, preserving font info, libs, layer order, features, kerning, groups, images and data.
`document::source_format::SourceFormatData` retains glyph-free UFO layer structure, residual font metadata and opaque image/data resources without storing another complete font document.
Compatibility projections still duplicate derived glyph payloads for remaining Norad algorithms.
They are not separate editable documents.
Native reload, live edits, proposals, experiments and browser edits cross the same scoped mutation boundary.

`document::axis` wraps pinned Babelfont coordinate conversion without exposing its types.
`document::var_model` wraps the fontdrasil variation backend used by Babelfont, retaining f64 values with rounding disabled.
`document::interpolation` reads canonical layer views directly, checks structure and paint order and interpolates exact advances, contours, anchors and six-coefficient component transforms using each glyph's own sources.
Component outlines resolve recursively at the same location, with explicit failures for missing or cyclic components.
The application reads these results instead of maintaining a second interpolation implementation.

`document::compile` builds a complete Babelfont snapshot and compiles it with fontc in Rust.
The same immutable OpenType bytes feed HarfRust shaping, Skrifa variable outlines and TTF export.
`document::compile_metadata` quantizes immutable canonical group and kerning inputs and still adapts remaining UFO metadata plus Designspace rules for that compiler.
The native preview worker coalesces pending edits and publishes only the current revision; slider changes reuse the compiled font.
The browser currently compiles synchronously and downloads exported bytes through a thin platform binding.
Application source controls live in `application/editor/sources.rs`; views dispatch commands and never perform font mutations themselves.

The [dependency and format decision](docs/variable-project-decision.md) records the upstream precision blocker, exact references, preservation policy and supported boundaries.
The [migration checklist](docs/babelfont-migration-checklist.md) tracks removal of the remaining Norad editing model while retaining source-format preservation.
The [source-format allowlist](docs/source-format-allowlist.md) assigns every supported UFO and Designspace field family to canonical, layer-preservation, source-format or filesystem ownership and records the explicit rejection boundary.

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
