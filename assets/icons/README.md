# Application icons

[icons.ufo](icons.ufo/) is the editable source for toolbar and control icons.
Open it in Runebender to change an outline; each glyph name is the name used by its button.
Its UPM and vertical metrics match Virtua Grotesk Regular: 1024 UPM, 768 ascender and cap height, 576 x-height, and -256 descender.
The existing icon outlines stay on their original coordinates, with the baseline at zero and the cap-height line at 768.
The glyph's advance width and height define its display frame, so keep the outline inside that frame.
The `plus`, `minus`, `grid`, `list`, `eye-*`, `invert`, and `sidebar-*` glyphs are the small status and navigation controls.
Their 768-unit frames correspond to 16 logical pixels in the footer.
Disclosure markers, menu symbols, the coordinate picker grid and dots, and the node resize grip also live here.
Colors and placement remain in the application, while each icon's shape comes from its UFO glyph.

The build script embeds the UFO's GLIF files for the native and browser editors.
Rebuild Runebender after saving an icon edit to see it in the application.
The app does not use a separate JSON copy of the icon paths.

Each icon has one stable Unicode Private Use Area code point.
Use the glyph name or code point when discussing an icon; new icons take the next unused PUA value, without renumbering existing glyphs.
The `select-all-layers` glyph is reserved for a future multi-layer editing tool; its code point is assigned even though the toolbar does not offer that mode yet.

| Code point | Glyph |
|---|---|
| `U+E000` | `close` |
| `U+E001` | `coordinate-dot` |
| `U+E002` | `coordinate-dot-selected` |
| `U+E003` | `coordinate-grid` |
| `U+E004` | `disclosure-bullet` |
| `U+E005` | `disclosure-closed` |
| `U+E006` | `disclosure-open` |
| `U+E007` | `duplicate` |
| `U+E008` | `duplicate-repeat` |
| `U+E009` | `exclude` |
| `U+E00A` | `eye-closed` |
| `U+E00B` | `eye-open` |
| `U+E00C` | `flip-h` |
| `U+E00D` | `flip-v` |
| `U+E00E` | `glyph-grid` |
| `U+E00F` | `grid` |
| `U+E010` | `hyperpen` |
| `U+E011` | `intersect` |
| `U+E012` | `invert` |
| `U+E013` | `knife` |
| `U+E014` | `lasso` |
| `U+E015` | `list` |
| `U+E016` | `measure` |
| `U+E017` | `menu-check` |
| `U+E018` | `menu-chevron` |
| `U+E019` | `menu-down` |
| `U+E01A` | `minus` |
| `U+E01B` | `node-resize` |
| `U+E01C` | `pen` |
| `U+E01D` | `plus` |
| `U+E01E` | `preview` |
| `U+E01F` | `rot-ccw` |
| `U+E020` | `rot-cw` |
| `U+E021` | `save` |
| `U+E022` | `save-as` |
| `U+E023` | `select` |
| `U+E024` | `select-all-layers` |
| `U+E025` | `select-menu` |
| `U+E026` | `shape-ellipse` |
| `U+E027` | `shape-metaball` |
| `U+E028` | `shape-rectangle` |
| `U+E029` | `shapes` |
| `U+E02A` | `shapes-menu` |
| `U+E02B` | `sidebar-closed` |
| `U+E02C` | `sidebar-open` |
| `U+E02D` | `subtract` |
| `U+E02E` | `text` |
| `U+E02F` | `text-ltr` |
| `U+E030` | `text-rtl` |
| `U+E031` | `union` |
| `U+E032` | `brush` |

Some of these designs started as Private Use Area glyphs in Virtua Grotesk.
This small UFO keeps the application's editable copies together without making it depend on the full font source.
