// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit in-memory UFO decoding boundary.
//!
//! Hosts with no filesystem, such as the web builds, receive font
//! data as `(path, bytes)` pairs over fetch or embedded in the
//! binary. This module assembles a font from those pairs.
//!
//! The supported subset is font metadata, feature text, groups, kerning and default-layer glyphs.
//! Extra layers, images and data are rejected explicitly so hosts never mistake a lossy import for
//! a preserved document.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use norad::designspace::DesignSpaceDocument;
use norad::{Font, Glyph};
use serde::Deserialize;

/// Parse a designspace document from XML text.
pub fn designspace_from_str(xml: &str) -> Result<DesignSpaceDocument, String> {
    crate::formats::designspace::parse(xml)
}

#[derive(Debug)]
/// A font assembled from memory plus the bookkeeping a host needs
/// to write changes back: which file each glyph came from.
pub struct UfoFiles {
    /// The assembled font, with every glyph loaded into its default layer.
    pub font: Font,
    /// glyph name → path relative to the UFO root ("glyphs/A_.glif").
    pub glif_paths: HashMap<String, String>,
}

/// Canonical imported project plus the host's glyph-to-file save bookkeeping.
#[derive(Debug)]
pub struct UfoProjectFiles {
    /// Canonical single-source project decoded at the UFO boundary.
    pub project: crate::document::project::Project,
    /// Glyph name to path relative to the UFO root, exactly as declared by `contents.plist`.
    pub glif_paths: HashMap<String, String>,
}

#[derive(Deserialize)]
struct EmbeddedGlifFont {
    info: EmbeddedGlifFontInfo,
    glyphs: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedGlifFontInfo {
    family_name: String,
    style_name: String,
    units_per_em: f64,
    ascender: f64,
    descender: f64,
    cap_height: f64,
    x_height: f64,
}

/// Assemble a font from UFO files given as (path, bytes) pairs. Paths
/// are relative to the UFO root ("fontinfo.plist",
/// "glyphs/contents.plist", "glyphs/A_.glif", ...).
pub fn font_from_files<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<Font, String> {
    ufo_from_files(files).map(|u| u.font)
}

/// Assemble a font from UFO files and keep the glyph-to-file
/// mapping, for hosts that save individual glifs back. Input is the
/// same as [`font_from_files`].
pub fn ufo_from_files<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<UfoFiles, String> {
    let files = validate_file_inventory(files)?;
    let mut font = Font::new();

    if let Some(bytes) = files.get("metainfo.plist") {
        font.meta = plist::from_bytes(bytes).map_err(|e| format!("metainfo.plist: {e}"))?;
    }
    if let Some(bytes) = files.get("fontinfo.plist") {
        font.font_info = plist::from_bytes(bytes).map_err(|e| format!("fontinfo.plist: {e}"))?;
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
    if let Some(bytes) = files.get("features.fea") {
        font.features = std::str::from_utf8(bytes)
            .map_err(|error| format!("features.fea: {error}"))?
            .to_owned();
    }

    validate_layer_contents(&files)?;

    // Default layer glyphs via contents.plist (glyph name → file).
    let contents: plist::Dictionary = match files.get("glyphs/contents.plist") {
        Some(bytes) => {
            plist::from_bytes(bytes).map_err(|e| format!("glyphs/contents.plist: {e}"))?
        }
        None => plist::Dictionary::new(),
    };
    let mut glif_paths = HashMap::new();
    let mut declared_paths = HashSet::new();
    let layer = font.default_layer_mut();
    for (name, value) in &contents {
        crate::document::canonical_metadata::validate_name(name)
            .map_err(|error| error.to_string())?;
        let file = value
            .as_string()
            .ok_or_else(|| format!("glyphs/contents.plist: {name:?} path is not a string"))?;
        if file.contains(['/', '\\']) || file.is_empty() || file == "." || file == ".." {
            return Err(format!("unsafe GLIF path for {name:?}: {file:?}"));
        }
        let path = format!("glyphs/{file}");
        if !declared_paths.insert(path.clone()) {
            return Err(format!("duplicate GLIF path {path:?}"));
        }
        let Some(bytes) = files.get(path.as_str()) else {
            return Err(format!("missing glif: {path}"));
        };
        let glyph = Glyph::parse_raw(bytes).map_err(|e| format!("{path}: {e}"))?;
        if glyph.name().as_str() != name {
            return Err(format!(
                "{path}: contents name {name:?} does not match GLIF name {:?}",
                glyph.name().as_str()
            ));
        }
        if glyph.image.is_some() {
            return Err(format!("{path}: glyph images are not supported in memory"));
        }
        layer.insert_glyph(glyph);
        glif_paths.insert(name.clone(), path);
    }
    for path in files.keys().copied() {
        if is_supported_root_file(path)
            || path == "glyphs/contents.plist"
            || declared_paths.contains(path)
        {
            continue;
        }
        if path.starts_with("images/") {
            return Err(format!("unsupported in-memory image payload: {path}"));
        }
        if path.starts_with("data/") {
            return Err(format!("unsupported in-memory data payload: {path}"));
        }
        if path.starts_with("glyphs/") && path.ends_with(".glif") {
            return Err(format!("unlisted GLIF payload: {path}"));
        }
        return Err(format!("unsupported in-memory UFO payload: {path}"));
    }
    Ok(UfoFiles { font, glif_paths })
}

/// Decode in-memory UFO files and immediately move the result into canonical Project ownership.
pub fn project_from_ufo_files<'a>(
    source_path: PathBuf,
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<UfoProjectFiles, String> {
    let decoded = ufo_from_files(files)?;
    let glif_paths = decoded.glif_paths;
    let project = crate::document::project::Project::from_ufo_boundary(source_path, &decoded.font)?;
    Ok(UfoProjectFiles {
        project,
        glif_paths,
    })
}

/// Decode an embedded font-info plus raw-GLIF JSON bundle into canonical Project ownership.
///
/// This is the browser demo's compact transport format, not a persisted font format.
pub fn project_from_embedded_glif_json(
    source_path: PathBuf,
    json: &str,
) -> Result<crate::document::project::Project, String> {
    let embedded: EmbeddedGlifFont =
        serde_json::from_str(json).map_err(|error| format!("embedded font JSON: {error}"))?;
    let mut font = Font::new();
    font.font_info.family_name = Some(embedded.info.family_name);
    font.font_info.style_name = Some(embedded.info.style_name);
    font.font_info.units_per_em = Some(
        embedded
            .info
            .units_per_em
            .try_into()
            .map_err(|error| format!("embedded font unitsPerEm: {error}"))?,
    );
    font.font_info.ascender = Some(embedded.info.ascender);
    font.font_info.descender = Some(embedded.info.descender);
    font.font_info.cap_height = Some(embedded.info.cap_height);
    font.font_info.x_height = Some(embedded.info.x_height);

    let mut names = HashSet::new();
    let layer = font.default_layer_mut();
    for (index, raw_glif) in embedded.glyphs.into_iter().enumerate() {
        let glyph = Glyph::parse_raw(raw_glif.as_bytes())
            .map_err(|error| format!("embedded glyph {index}: {error}"))?;
        let name = glyph.name().as_str();
        crate::document::canonical_metadata::validate_name(name)
            .map_err(|error| error.to_string())?;
        if !names.insert(name.to_owned()) {
            return Err(format!("duplicate embedded glyph name {name:?}"));
        }
        if glyph.image.is_some() {
            return Err(format!(
                "embedded glyph {name:?}: images are not supported in memory"
            ));
        }
        layer.insert_glyph(glyph);
    }
    crate::document::project::Project::from_ufo_boundary(source_path, &font)
}

fn validate_file_inventory<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<HashMap<&'a str, &'a [u8]>, String> {
    let mut validated = HashMap::new();
    for (path, bytes) in files {
        if !safe_relative_path(path) {
            return Err(format!("unsafe UFO path {path:?}"));
        }
        if validated.insert(path, bytes).is_some() {
            return Err(format!("duplicate UFO path {path:?}"));
        }
    }
    Ok(validated)
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_layer_contents(files: &HashMap<&str, &[u8]>) -> Result<(), String> {
    let Some(bytes) = files.get("layercontents.plist") else {
        return Ok(());
    };
    let value: plist::Value =
        plist::from_bytes(bytes).map_err(|error| format!("layercontents.plist: {error}"))?;
    let entries = value
        .as_array()
        .ok_or("layercontents.plist: expected an array")?;
    if entries.len() != 1 {
        return Err("in-memory UFO import supports only the default layer".into());
    }
    let entry = entries[0]
        .as_array()
        .filter(|entry| entry.len() == 2)
        .ok_or("layercontents.plist: malformed layer entry")?;
    let name = entry[0]
        .as_string()
        .ok_or("layercontents.plist: layer name is not a string")?;
    let directory = entry[1]
        .as_string()
        .ok_or("layercontents.plist: layer directory is not a string")?;
    if name != "public.default" || directory != "glyphs" {
        return Err(format!(
            "unsupported in-memory layer {name:?} at {directory:?}"
        ));
    }
    Ok(())
}

fn is_supported_root_file(path: &str) -> bool {
    matches!(
        path,
        "metainfo.plist"
            | "fontinfo.plist"
            | "lib.plist"
            | "groups.plist"
            | "kerning.plist"
            | "features.fea"
            | "layercontents.plist"
    )
}

/// Serialize one glyph to glif XML bytes, for hosts saving over
/// HTTP instead of a filesystem.
pub fn glif_bytes(glyph: &Glyph) -> Result<Vec<u8>, String> {
    glyph.encode_xml().map_err(|e| format!("encode glif: {e}"))
}

/// Serialize a font's kerning to kerning.plist XML bytes.
pub fn kerning_plist_bytes(font: &Font) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    plist::to_writer_xml(&mut out, &font.kerning).map_err(|e| format!("kerning: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const METAINFO: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>creator</key><string>org.linebender.runebender.tests</string>
<key>formatVersion</key><integer>3</integer>
</dict></plist>"#;
    const FONTINFO: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>familyName</key><string>MemTest</string>
<key>styleName</key><string>Regular</string>
<key>unitsPerEm</key><integer>1000</integer>
<key>ascender</key><integer>800</integer>
<key>descender</key><integer>-200</integer>
</dict></plist>"#;
    const LIB: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>com.linebender.test</key><string>preserved</string>
</dict></plist>"#;
    const GROUPS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>public.kern1.Left</key><array><string>A</string></array>
</dict></plist>"#;
    const KERNING: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>public.kern1.Left</key><dict><key>A</key><integer>-25</integer></dict>
</dict></plist>"#;
    const LAYERCONTENTS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><array>
<array><string>public.default</string><string>glyphs</string></array>
</array></plist>"#;
    const CONTENTS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>A</key><string>A_.glif</string>
</dict></plist>"#;
    const GLIF: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
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
    const FEATURES: &[u8] = b"feature liga { sub A A by A; } liga;\n";

    fn standard_files() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("metainfo.plist", METAINFO),
            ("fontinfo.plist", FONTINFO),
            ("lib.plist", LIB),
            ("groups.plist", GROUPS),
            ("kerning.plist", KERNING),
            ("features.fea", FEATURES),
            ("layercontents.plist", LAYERCONTENTS),
            ("glyphs/contents.plist", CONTENTS),
            ("glyphs/A_.glif", GLIF),
        ]
    }

    #[test]
    fn builds_font_from_memory() {
        let font = font_from_files([
            ("fontinfo.plist", FONTINFO),
            ("glyphs/contents.plist", CONTENTS),
            ("glyphs/A_.glif", GLIF),
        ])
        .expect("font builds");
        assert_eq!(font.font_info.family_name.as_deref(), Some("MemTest"));
        let a = font.get_glyph("A").expect("glyph A");
        assert_eq!(a.width, 600.0);
        assert_eq!(a.contours.len(), 1);
    }

    #[test]
    fn imports_standard_memory_files_into_canonical_ownership() {
        let imported = project_from_ufo_files(PathBuf::from("Memory.ufo"), standard_files())
            .expect("project builds");
        let project = imported.project;
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        assert_eq!(
            imported.glif_paths.get("A").map(String::as_str),
            Some("glyphs/A_.glif")
        );
        assert_eq!(project.document_layer("A", &layer).unwrap().width(), 600.0);
        assert_eq!(
            project
                .document_layer("A", &layer)
                .unwrap()
                .codepoints()
                .collect::<Vec<_>>(),
            ['A']
        );
        assert_eq!(
            project.document_feature_text(source),
            Some(std::str::from_utf8(FEATURES).unwrap())
        );
        let metadata = project.document_font_metadata(source).unwrap();
        assert_eq!(metadata.groups().get("public.kern1.Left").unwrap(), &["A"]);
        assert_eq!(metadata.raw_kerning()["public.kern1.Left"]["A"], -25.0);
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .names
                .family_name
                .as_deref(),
            Some("MemTest")
        );
        let snapshot = project.encode_ufo_source(source).unwrap();
        assert_eq!(
            snapshot.meta.creator.as_deref(),
            Some("org.linebender.runebender.tests")
        );
        assert_eq!(
            snapshot.lib["com.linebender.test"].as_string(),
            Some("preserved")
        );
        assert!(!project.is_modified());
    }

    #[test]
    fn embedded_glif_json_enters_project_canonically() {
        let project = project_from_embedded_glif_json(
            PathBuf::from("VirtuaGrotesk-Regular.ufo"),
            include_str!("../../web/demo-font.json"),
        )
        .expect("the browser demo imports");
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        assert_eq!(project.glyph_names().count(), 863);
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .names
                .family_name
                .as_deref(),
            Some("Virtua Grotesk")
        );
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .metrics
                .units_per_em,
            Some(1024.0)
        );
        let a = project.document_layer("A", &layer).unwrap();
        assert_eq!(a.width(), 716.0);
        assert_eq!(a.codepoints().collect::<Vec<_>>(), ['A']);
        assert!(!project.is_modified());
    }

    #[test]
    fn rejects_unsafe_and_duplicate_memory_paths() {
        let duplicate = ufo_from_files([
            ("glyphs/contents.plist", CONTENTS),
            ("glyphs/contents.plist", CONTENTS),
        ])
        .unwrap_err();
        assert!(duplicate.contains("duplicate UFO path"));
        let unsafe_path = ufo_from_files([("../fontinfo.plist", FONTINFO)]).unwrap_err();
        assert!(unsafe_path.contains("unsafe UFO path"));
    }

    #[test]
    fn rejects_mismatched_and_duplicate_glif_declarations() {
        const MISMATCHED: &[u8] = br#"<plist version="1.0"><dict>
<key>B</key><string>A_.glif</string>
</dict></plist>"#;
        let mismatch = ufo_from_files([
            ("glyphs/contents.plist", MISMATCHED),
            ("glyphs/A_.glif", GLIF),
        ])
        .unwrap_err();
        assert!(mismatch.contains("does not match GLIF name"));

        const DUPLICATE: &[u8] = br#"<plist version="1.0"><dict>
<key>A</key><string>A_.glif</string>
<key>B</key><string>A_.glif</string>
</dict></plist>"#;
        let duplicate = ufo_from_files([
            ("glyphs/contents.plist", DUPLICATE),
            ("glyphs/A_.glif", GLIF),
        ])
        .unwrap_err();
        assert!(duplicate.contains("duplicate GLIF path"));
    }

    #[test]
    fn rejects_payloads_the_memory_boundary_cannot_preserve() {
        const EXTRA_LAYERS: &[u8] = br#"<plist version="1.0"><array>
<array><string>public.default</string><string>glyphs</string></array>
<array><string>sketch</string><string>glyphs.sketch</string></array>
</array></plist>"#;
        let layer = ufo_from_files([("layercontents.plist", EXTRA_LAYERS)]).unwrap_err();
        assert!(layer.contains("only the default layer"));

        for (path, expected) in [
            ("images/reference.png", "image payload"),
            ("data/com.example/payload", "data payload"),
            ("glyphs/unlisted.glif", "unlisted GLIF payload"),
        ] {
            let error = ufo_from_files([(path, b"payload".as_slice())]).unwrap_err();
            assert!(error.contains(expected), "{error:?}");
        }

        const IMAGE_GLIF: &[u8] = br#"<glyph name="A" format="2">
<advance width="600"/>
<image fileName="reference.png"/>
</glyph>"#;
        let image = ufo_from_files([
            ("glyphs/contents.plist", CONTENTS),
            ("glyphs/A_.glif", IMAGE_GLIF),
        ])
        .unwrap_err();
        assert!(image.contains("glyph images are not supported"));
    }

    #[test]
    fn glif_bytes_roundtrip() {
        let glif = br#"<?xml version="1.0" encoding="UTF-8"?>
<glyph name="B" format="2">
<advance width="500"/>
<outline>
<contour>
<point x="0" y="0" type="line"/>
<point x="10" y="0" type="line"/>
<point x="10" y="10" type="line"/>
</contour>
</outline>
</glyph>"#;
        let mut glyph = Glyph::parse_raw(glif).unwrap();
        crate::outline::glyph_ops::set_points(&mut glyph, &[((0, 0), (5.0, 5.0))]);
        let bytes = glif_bytes(&glyph).unwrap();
        let back = Glyph::parse_raw(&bytes).unwrap();
        assert_eq!(back.contours[0].points[0].x, 5.0);
        assert_eq!(back.width, 500.0);
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
