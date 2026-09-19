# Babelfont canonical constructor boundary

New fonts now begin as typed canonical document values instead of a populated Norad font.
The checked-in GF Latin Core template creates 324 empty canonical glyph layers, typed names and metrics, exact advances and Unicode scalars under stable source identity `SourceId(0)`.
Only after canonical construction does Project derive the compatibility `Master` and glyph-free Norad persistence template.
New documents start dirty so their first save writes the canonical template.

The in-memory UFO path is now an explicit decoder boundary.
It accepts `metainfo.plist`, `fontinfo.plist`, `lib.plist`, `groups.plist`, `kerning.plist`, `features.fea`, the default `layercontents.plist`, `glyphs/contents.plist` and its declared GLIF files.
Norad decodes that supported source-format subset transiently, after which Project imports the data into canonical ownership and derives its compatibility projection.
Imported documents start clean and retain the exact glyph-to-GLIF paths required by browser save bookkeeping.

The boundary validates the complete file inventory before publishing a Project.
It rejects duplicate or unsafe paths, missing or unlisted GLIF files, duplicate GLIF targets, mismatched contents and GLIF names, non-default layers, glyph images, image payloads, data payloads and other unsupported files.
This preserves the browser's existing default-layer support limit while replacing silent loss with an explicit error.

Focused tests cover canonical new-font construction, save and reopen, standard in-memory metadata and glyph import, exact path bookkeeping and every unsupported-payload class above.
