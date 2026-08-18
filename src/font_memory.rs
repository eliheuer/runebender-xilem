// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Build a norad [`Font`] from in-memory UFO files — for hosts with
//! no filesystem (the web builds), where font data arrives as
//! (path, bytes) pairs over fetch or embedded in the binary.
//!
//! This is a pragmatic subset of UFO loading: fontinfo, lib, groups,
//! kerning, and the default layer's glyphs. Extra layers, images,
//! and data files are ignored for now.

use std::collections::HashMap;

use norad::designspace::DesignSpaceDocument;
use norad::{Font, Glyph};

/// Parse a designspace document from XML text.
pub fn designspace_from_str(xml: &str) -> Result<DesignSpaceDocument, String> {
    quick_xml::de::from_str(xml).map_err(|e| format!("designspace: {e}"))
}

/// Assemble a font from UFO files given as (path, bytes) pairs. Paths
/// are relative to the UFO root ("fontinfo.plist",
/// "glyphs/contents.plist", "glyphs/A_.glif", ...).
pub fn font_from_files<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<Font, String> {
    let files: HashMap<&str, &[u8]> = files.into_iter().collect();
    let mut font = Font::new();

    if let Some(bytes) = files.get("fontinfo.plist") {
        font.font_info =
            plist::from_bytes(bytes).map_err(|e| format!("fontinfo.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("lib.plist") {
        let value: plist::Dictionary =
            plist::from_bytes(bytes).map_err(|e| format!("lib.plist: {e}"))?;
        font.lib = value;
    }
    if let Some(bytes) = files.get("groups.plist") {
        font.groups = plist::from_bytes(bytes).map_err(|e| format!("groups.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("kerning.plist") {
        font.kerning = plist::from_bytes(bytes).map_err(|e| format!("kerning.plist: {e}"))?;
    }

    // Default layer glyphs via contents.plist (glyph name → file).
    let contents: HashMap<String, String> = match files.get("glyphs/contents.plist") {
        Some(bytes) => {
            plist::from_bytes(bytes).map_err(|e| format!("contents.plist: {e}"))?
        }
        None => HashMap::new(),
    };
    let layer = font.default_layer_mut();
    for file in contents.values() {
        let path = format!("glyphs/{file}");
        let Some(bytes) = files.get(path.as_str()) else {
            return Err(format!("missing glif: {path}"));
        };
        let glyph = Glyph::parse_raw(bytes).map_err(|e| format!("{path}: {e}"))?;
        layer.insert_glyph(glyph);
    }
    Ok(font)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_font_from_memory() {
        let fontinfo = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>familyName</key><string>MemTest</string>
<key>unitsPerEm</key><integer>1000</integer>
<key>ascender</key><integer>800</integer>
<key>descender</key><integer>-200</integer>
</dict></plist>"#;
        let contents = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>A</key><string>A_.glif</string>
</dict></plist>"#;
        let glif = br#"<?xml version="1.0" encoding="UTF-8"?>
<glyph name="A" format="2">
<advance width="600"/>
<unicode hex="0041"/>
<outline>
<contour>
<point x="0" y="0" type="line"/>
<point x="100" y="0" type="line"/>
<point x="50" y="700" type="line"/>
</contour>
</outline>
</glyph>"#;
        let font = font_from_files([
            ("fontinfo.plist", fontinfo.as_slice()),
            ("glyphs/contents.plist", contents.as_slice()),
            ("glyphs/A_.glif", glif.as_slice()),
        ])
        .expect("font builds");
        assert_eq!(font.font_info.family_name.as_deref(), Some("MemTest"));
        let a = font.get_glyph("A").expect("glyph A");
        assert_eq!(a.width, 600.0);
        assert_eq!(a.contours.len(), 1);
    }

    #[test]
    fn parses_designspace_text() {
        let ds = r#"<?xml version='1.0' encoding='UTF-8'?>
<designspace format="4.0">
  <axes><axis name="Weight" tag="wght" minimum="400" default="400" maximum="700"/></axes>
  <sources>
    <source familyname="T" stylename="Regular" filename="T-Regular.ufo">
      <location><dimension name="Weight" xvalue="400"/></location>
    </source>
  </sources>
</designspace>"#;
        let doc = designspace_from_str(ds).expect("parses");
        assert_eq!(doc.axes.len(), 1);
        assert_eq!(doc.sources.len(), 1);
    }
}
