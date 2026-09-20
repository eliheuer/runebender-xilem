# UFO and Designspace preservation allowlist

Status: **enforced M12/M13 boundary contract**.
This inventory describes the source-format fields accepted by the filesystem UFO/Designspace adapter and assigns each field family one authoritative owner.
Anything not listed here must either remain opaque under the listed boundary payload or fail before Project construction.
It must never disappear during load, canonical edit, save, Save As or reopen.

## Ownership labels

- **Canonical** means the live document owns the editable value and export reconstructs its UFO or Designspace representation.
- **Layer preservation** means the value is attached to a stable glyph-layer or object identity because Babelfont cannot represent it faithfully.
- **SourceFormatData** means the value is source-wide format data required for lossless serialization but is not editable canonical state.
  The record is glyph-free and does not retain a complete Norad font.
- **Filesystem preservation** means raw bytes or exact paths outside Norad's supported object model.

## UFO allowlist

| UFO field family | Current authoritative owner | Export rule |
|---|---|---|
| Source destination and dirty/save status | Project source shell | Save and Save As publish only to checked destinations; source removal never deletes the UFO. |
| `metainfo.plist`: creator, format version and minor version | SourceFormatData | Preserve the decoded values exactly. |
| `fontinfo.plist`: family/style, copyright/trademark, designer/manufacturer/license/description/version/unique/sample/PostScript/typographic/WWS names | Canonical `CanonicalFontInfo::names` | Clear from the preservation payload and reconstruct from the canonical value. |
| `fontinfo.plist`: units per em, ascender, descender, x-height, cap height and italic angle | Canonical `CanonicalFontInfo::metrics` | Retain exact `f64` values and reject nonfinite values or negative units per em. |
| `fontinfo.plist`: supported hhea and OS/2 metrics, head flags, embedding and selection flags, weight/width class, vendor ID, note and major/minor version | Canonical `CanonicalFontInfo` OpenType fields | Clear from the preservation payload and reconstruct from the canonical value. |
| Every other field represented by Norad 0.18.4 `FontInfo` | SourceFormatData | Preserve the typed value unchanged; representative unowned name and PostScript fields are exercised by the allowlist regression. |
| `features.fea` | Canonical per-source metadata | Preserve exact text; Save As also relocates recursively resolved relative include files. |
| `groups.plist` and `kerning.plist` | Canonical per-source metadata | Preserve all valid groups, member order, typed pair spelling and exact finite `f64` values. |
| `lib.plist` entries for `public.skipExportGlyphs` and `public.openTypeCategories` that name loaded glyphs | Canonical per-source glyph metadata for semantics; SourceFormatData for source ordering | Canonical values control edits; the glyph-free boundary record preserves list order, source spelling and unknown-name entries. |
| Entries in those two standard dictionaries that do not name a loaded glyph, plus every other font-lib key | SourceFormatData | Preserve exact plist values and merge canonical entries without overwriting residual entries. |
| Layer order, layer names, glyph-directory paths, layer color and layer lib | SourceFormatData, addressed by canonical `LayerId` | Preserve order and exact persistence paths while canonical source/layer structure owns editable association. |
| Glyph name and layer membership | Canonical Project glyph/source structure | Structural operations update stable identities and rebuild the corresponding GLIF set. |
| Contours, point coordinates/types/smooth state, components, anchors and their paint order | Canonical Babelfont geometry | Export from canonical geometry without a persistent UFO glyph owner. |
| Horizontal and vertical advances, Unicode order, note, guidelines, image placement and residual glyph lib | Layer preservation | Retain exact values beside the stable layer identity and merge known keys only at export. |
| Contour, point, component, anchor and guideline identifiers, names, colors, transforms and object libs | Layer preservation keyed by stable object identity | Reorder moves metadata with identity; replacement retains metadata only through an explicit mapping. |
| Mark color, metrics formulas, composition recipe, metaballs, smart-component values and poles, HOI intermediates, editable hyperbezier kind and mask encoding | Typed or validated layer preservation | Decode known keys once, retain their exact supported source representation and write each key once at export. |
| `images/` and `data/` resource names and bytes | SourceFormatData | Preserve exact relative names and bytes; image placement is a separate layer value. |
| Exact `contents.plist` GLIF filenames | Filesystem preservation | Restore validated relative paths after staging and verify the staged font reloads with them. |
| Regular files outside Norad-managed UFO files, `images/` and `data/` | Filesystem preservation | Copy bytes unchanged into the staged UFO. |

The enforced regression `populated_ufo_allowlist_survives_canonical_edit_save_and_reopen` populates every row-level storage family that is not already exhaustively covered by the canonical font-info, glyph preservation and variable-project suites.
It compares the complete Norad-supported UFO value after a canonical edit/save and separately checks opaque filesystem bytes.
The existing filesystem regression `project_save_preserves_filesystem_payload_and_ufo_paths` additionally checks exact custom GLIF paths, auxiliary layer paths and opaque filesystem bytes.
The glyph-metadata unit test `canonical_ufo_boundary_preserves_unrelated_font_metadata` proves that edits replace canonical font-lib semantics while unknown-name entries survive in their original order.

## Designspace allowlist

Every accepted Designspace field is canonical; there is no opaque XML fallback.
The checked XML reader rejects unknown structure before Norad can ignore it.

| Designspace field family | Canonical owner |
|---|---|
| Format version | `CanonicalDesignspace` |
| Continuous axes: name, tag, explicit or implicit minimum/maximum, default, hidden flag, ordered map and localized labels | `CanonicalAxis` |
| Full sources: order, family/style/name, filename and sparse user-or-design location | `SourceDescriptor` plus `CanonicalLocation` |
| Layer-only sparse sources: order, owning UFO/layer, family/style/name and location | `SparseSourceDescriptor` plus `SourceOrderEntry` |
| Instances: order, family/style/name, filename, PostScript name, style-map names, location and instance lib | `CanonicalInstance` |
| Rules: processing order, names, ordered condition sets, bounds and substitutions | `CanonicalRule` |
| Top-level lib | `CanonicalDesignspace` opaque plist dictionary |
| Presence of an empty axis-mappings container | `CanonicalDesignspace` preservation flag |

`tests/canonical_designspace.rs` checks exact import/export equality for this accepted structure, stable identities and the explicit rejection boundary.
`formats::designspace` validates the XML element and attribute allowlist before typed decoding, and every persisted coordinate must round-trip through Norad's `f32` representation.

## Intentional rejection boundary

The adapter intentionally returns an error for these inputs instead of attempting a lossy save:

- unknown Designspace elements or attributes outside plist `lib` contents;
- nonempty cross-axis mappings and discrete-axis `values`;
- anisotropic or ambiguous locations, unknown or duplicate dimensions, missing values and nonfinite coordinates;
- Designspace numeric spellings that cannot round-trip through the current Norad codec;
- duplicate axis names/tags, duplicate full-source files or locations, a missing mapped-default source, and layer sources without their owning UFO/layer;
- malformed canonical UFO metadata, including invalid group or kerning names, nonfinite kerning, invalid font metrics, duplicate/non-string skip-export values and non-string OpenType category values;
- UFO symbolic links, non-regular filesystem entries, unsafe preserved GLIF paths and overlapping or aliased export destinations;
- unsupported imported-format fields already documented by the Glyphs, compiled-font and Python Babelfont adapters.

Rust Babelfont JSON remains unsupported.
The Python Babelfont directory importer is a separate checked conversion contract and never rewrites its source package.
