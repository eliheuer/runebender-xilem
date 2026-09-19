# Babelfont document field and identity ownership

Status: **M01 contract — implementation incomplete**.
Reviewed against upstream Babelfont `29bdedbbfa7d3150b651dbd7c94fce6b79677ca4`, Norad 0.13.0 and Runebender `314aa3235c372ed8d5fef7a2cddb8be3a07ad1da`.
This contract defines the destination model for the migration.
It does not make the current Norad mirrors or index-based projection safe.

## Rules

Every known editable field has one authoritative value in the document.
Babelfont owns values it represents faithfully and Runebender owns typed extensions for precision or metadata that Babelfont does not represent faithfully.
A source-format payload may preserve unknown data, but it must not also retain a mutable copy of a known field.
Changing a known field updates its one owner and leaves unrelated opaque data byte-for-byte or value-for-value intact.
Norad is a UFO/Designspace codec at import and export, not an editable document or metadata cache.
Compiler snapshots quantize copies and never write quantized values back into the document.

An imported unsupported field is either preserved opaquely with a documented enclosing object, or rejected before editing begins.
It is never silently dropped.
If a later version recognizes an opaque field, import promotes it to typed ownership once and removes that key from the opaque payload.

## Field ownership

| Field family | Authoritative owner | Exact or preserving extension | Mutation and export rule |
|---|---|---|---|
| Contour topology, point coordinates/types/smooth state, component references and anchor position/name | Babelfont glyph layers | Stable object identities and UFO metadata described below | Document geometry transactions mutate Babelfont objects directly. UFO export combines each object with metadata selected by identity. |
| Horizontal and vertical advances | Runebender layer metrics as `f64` | Both values remain exact even though Babelfont layer width is `f32` and has no equivalent authoritative vertical advance | Every metric query, interpolation and edit uses the extension. Babelfont width is a derived compatibility value until upstream can represent the exact value. Compilation performs checked target-format quantization. |
| Component affine transform | Runebender exact six-coefficient `f64` matrix | The corresponding Babelfont decomposed transform is derived for Babelfont algorithms | A matrix edit writes the exact matrix and refreshes the derived decomposition. A Babelfont transform operation writes one newly composed exact matrix. Repeated reads or no-op saves never decompose and recompose the exact value. |
| Contour, point, component and anchor UFO identifier and lib | Runebender object metadata keyed by stable typed identity | Identifier is optional; lib is an optional property list | Insert allocates a fresh identity. Reorder moves the object with its identity. Delete removes its metadata. Duplicate and copy/paste allocate fresh identities and copy user metadata, clearing or regenerating UFO identifiers to preserve uniqueness. |
| Point name, anchor color, component lib and contour lib | Runebender object metadata keyed by stable typed identity | Preserve values absent from, or not faithfully round-tripped by, the selected Babelfont core | An edit targets the object identity. Topology replacement keeps metadata only for objects explicitly mapped by the operation. |
| Glyph Unicode values, note, image placement, glyph guidelines and glyph lib | Runebender typed glyph/layer metadata | Image includes file name, color and exact affine matrix. Guidelines include geometry, name, color, identifier and lib. Unknown glyph-lib keys remain opaque after known Runebender keys are decoded. | Glyph rename moves the metadata with the glyph identity. Layer copy duplicates values into a new layer identity. Save encodes known keys once and merges untouched opaque keys. |
| Glyph name | Project glyph table keyed by stable `GlyphId` | Name is a mutable indexed property, not identity | Rename changes the name index atomically and updates components, kerning, groups, glyph order and open selections by identity. Undo restores the former name without changing `GlyphId`. |
| Export/category flags, mark color, metrics formulas, metaballs, masks and HOI data | Runebender typed glyph extensions | UFO lib keys are boundary encodings | Live tools read and write typed values. Only explicit bake/convert operations replace their special source data with ordinary geometry. |
| Glyph order | Project font metadata as an ordered list of `GlyphId` values | Unknown order entries that do not name an imported glyph are preserved separately when the source contract allows them | Rename does not reorder. Insert chooses one documented position. Delete removes only the deleted identity. |
| Font names, metrics, units-per-em, copyright and other supported font-info fields | Project font metadata | Values unsupported by Babelfont remain typed where Runebender edits them and opaque otherwise | UI and headless edits mutate the project field. UFO export emits the project value and retains unrelated font-info data. |
| Font and layer guidelines | Project source/layer metadata | Exact geometry, name, color, identifier and lib | Reorder moves the complete guide identity. Copy allocates a new identity and a unique UFO identifier. |
| Groups and kerning | Project font-wide data with `f64` kerning values | Fractional values remain exact despite Babelfont master's `i16` kerning | Renames and group edits update references atomically. Compilation quantizes only its immutable snapshot and reports out-of-range values. |
| Feature text | Project default-source feature value plus per-source preservation values | Includes and original per-source text remain source-bound | The default source supplies live shared feature editing. Other sources keep their text unless explicitly edited. Draft checking is non-mutating. |
| Axes and mappings | Project variable metadata using exact `f64` user/design coordinates | Stable `AxisId`; unsupported Designspace constructs are rejected as documented | Display order is separate from identity. Coordinate conversion uses the pinned fontdrasil backend. Serialization rounding is checked before commit. |
| Sources and locations | Project source records keyed by `SourceId` | Source path, source name, exact normalized/design location, layer participation and source lib data | Reorder changes display order only. Removal detaches the record without deleting its source. Undo restores the same `SourceId`. |
| Instances and Designspace rules | Project variable metadata keyed by `InstanceId` and `RuleId` | Names, locations, style-linking metadata and opaque supported XML data remain attached to identity | Reorder and rename preserve identity. Compilation and Designspace export read the same values. Unsupported XML is rejected before it can be lost. |
| Layer name, source association, background status, intermediate location, color and layer lib | Project glyph layer record keyed by `LayerId` | Source-format layer identifiers and unknown layer data are extensions on that layer record | Rename changes a property while references keep `LayerId`. Promotion or source reassignment is an explicit structural transaction. |
| Images and data stores | Project source resources | Bytes and relative names remain exact; glyph image placement is separate typed metadata | Save copies or writes resources without decoding them. Removal of a glyph placement does not remove source image bytes unless an explicit resource command does so. |
| Unknown font, source, instance, glyph, layer and object metadata | Opaque payload owned by the narrowest stable enclosing identity | Original typed plist/JSON/XML representation where the supported format contract permits it | Known keys are removed before storage. Structural operations define whether the enclosing payload moves, copies or is discarded. Export merges it without overwriting typed fields. |

Babelfont `format_specific` is not a general second owner for fields in this table.
It may carry an adapter token required by a Babelfont algorithm or immutable compiler snapshot, but live Runebender mutations go through the typed document extension.

## Identity model

The document uses distinct types for `AxisId`, `SourceId`, `InstanceId`, `RuleId`, `GlyphId`, `LayerId`, `ContourId`, `PointId`, `ComponentId`, `AnchorId` and `GuideId`.
IDs are opaque and unique within one document session.
Types prevent an anchor or point ID from accidentally indexing contour metadata.
Source and layer identity already exist, but `LayerId` must stop using mutable layer names as identity before layer rename is exposed.

An imported valid source identifier seeds the corresponding document ID mapping.
Objects without a source identifier receive a fresh document ID without inventing an on-disk identifier.
The document preserves the difference between “no UFO identifier” and “generated document identity.”
Export reuses an imported identifier when it is still unique.
It writes a new UFO identifier only when the operation requires one, such as an object with a lib or a duplicate that would otherwise collide.

Babelfont layers and shapes carry their document IDs through a private adapter token so immutable Babelfont clones keep identity.
The token is not a source-format field and is removed from external Babelfont or compiler serialization.
Exact metadata maps use the typed IDs as keys and never infer ownership from current array position.

### Operation rules

| Operation | Identity behavior |
|---|---|
| Coordinate, smooth/type, name, color, transform or metadata edit | Retain the edited object's identity. |
| Reorder or cyclic rotation | Move objects with their identities; no metadata matching pass runs afterward. A closed contour's start-point rotation does not rename points. |
| Insert | Allocate a fresh typed identity and start with no UFO identifier unless copied user data requires one. |
| Delete | Remove the identity and all metadata owned by it in the same transaction. Undo restores the same identity and metadata. |
| Duplicate or copy/paste | Allocate fresh identities recursively. Copy user-facing metadata, but never copy a uniqueness-constrained UFO identifier verbatim. |
| Glyph or layer rename | Retain `GlyphId` or `LayerId`; update name indexes and references atomically. |
| Replace topology | The operation must supply an explicit old-to-new identity map. Unmapped old metadata is deleted and unmapped new objects receive fresh identities. Positional guessing is forbidden. |
| Boolean, simplify, overlap removal or conversion | Treat generated topology as replacement unless the algorithm can prove an explicit correspondence. Preserve glyph/layer metadata and special editable source data according to that command's contract. |
| Undo/redo | Restore values, identity maps and metadata together. A rejected replay changes none of them. |
| Source/layer reorder | Retain all IDs and update ordered indexes only. |
| Source/layer removal and undo | Detach and restore the same IDs. Source files remain on disk. |
| Experimental version or proposal fork | Clone canonical identities into an isolated version so conflicts compare the same logical objects. New objects created in the branch use branch-local fresh IDs; accepted apply assigns collision-free root identities atomically. |

## Canonical and boundary snapshots

A history or experiment snapshot contains canonical Babelfont values, exact extensions, identity maps and the minimum project metadata in its transaction scope.
It does not contain a `norad::Glyph`, `norad::Font` or a serialized round trip.
A compiler snapshot contains only values needed by fontc plus checked quantized copies.
A UFO/Designspace export snapshot materializes codec objects from the canonical document and preserving payloads, then discards them after writing.

The existing `VariableGlyph.layers: BTreeMap<LayerId, norad::Glyph>`, `VariableData.templates`, Master fonts, source guards and positional `project_layer` restoration violate this destination contract and remain tracked migration work.
M01 must implement typed extensions and identity-aware projection before M02 exposes direct topology transactions.
