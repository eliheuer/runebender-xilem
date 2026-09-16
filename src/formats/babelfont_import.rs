// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Basic, read-only import of single-master `.babelfont` directory packages.
//! The package format follows simoncozens/babelfont's `convertors/nfsf.py`.
//! Import brings outlines, components, anchors, widths, Unicode, names, metrics,
//! and kerning into a UFO. Variable sources and additional layers are rejected.
//! Saving the imported font writes UFO; it never rewrites the source package.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Info {
    #[serde(default = "default_upm")]
    upm: f64,
    masters: Vec<SourceMaster>,
    #[serde(default)]
    axes: Vec<Value>,
    #[serde(default)]
    instances: Vec<Value>,
    #[serde(default)]
    first_kern_groups: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    second_kern_groups: BTreeMap<String, Vec<String>>,
}

fn default_upm() -> f64 {
    1000.0
}
fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct SourceMaster {
    id: String,
    name: Value,
    #[serde(default)]
    metrics: BTreeMap<String, f64>,
    #[serde(default)]
    kerning: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct SourceGlyph {
    name: String,
    #[serde(default)]
    codepoints: Vec<u32>,
    #[serde(default = "yes")]
    exported: bool,
}

#[derive(Deserialize)]
struct Layer {
    #[serde(rename = "_master")]
    master: String,
    #[serde(default)]
    width: f64,
    #[serde(default)]
    shapes: Vec<Shape>,
    #[serde(default)]
    anchors: Vec<SourceAnchor>,
    #[serde(default, rename = "isBackground")]
    is_background: bool,
    #[serde(default)]
    location: Option<Vec<f64>>,
}

#[derive(Deserialize)]
struct SourceAnchor {
    name: String,
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
struct Shape {
    #[serde(rename = "ref")]
    reference: Option<String>,
    transform: Option<[f64; 6]>,
    #[serde(default)]
    nodes: Vec<Value>,
    #[serde(default = "yes")]
    closed: bool,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn localized(value: &Value) -> Option<String> {
    value
        .as_str()
        .or_else(|| value.get("dflt").and_then(Value::as_str))
        .or_else(|| value.as_object()?.values().find_map(Value::as_str))
        .map(str::to_owned)
}

/// Read a single-master Babelfont package as an editable UFO, without writing files.
///
/// Requires `info.json`, `names.json`, `glyphs.json`, and the indexed `.nfsglyph`
/// files. Rejects multiple masters, axes, instances, additional/background layers,
/// invalid nodes, missing components, and malformed metadata. Features containing
/// external includes are rejected because their paths cannot survive UFO import.
/// Guides, hints, production names, and application-specific metadata are not imported.
pub fn import_babelfont(path: &Path) -> Result<Font, String> {
    let info: Info = read_json(&path.join("info.json"))?;
    if info.masters.len() != 1 || !info.axes.is_empty() || !info.instances.is_empty() {
        return Err(
            "Babelfont import currently supports one master without axes or instances".into(),
        );
    }
    if !info.upm.is_finite() || info.upm <= 0.0 {
        return Err("Babelfont units per em must be positive and finite".into());
    }
    let names: BTreeMap<String, Value> = read_json(&path.join("names.json"))?;
    let source_glyphs: Vec<SourceGlyph> = read_json(&path.join("glyphs.json"))?;
    let master = &info.masters[0];
    let mut font = Font::default();
    font.font_info.family_name = names.get("familyName").and_then(localized);
    font.font_info.style_name = localized(&master.name);
    font.font_info.units_per_em = Some(
        info.upm
            .try_into()
            .map_err(|e| format!("units per em: {e}"))?,
    );
    font.font_info.ascender = master.metrics.get("ascender").copied();
    font.font_info.descender = master.metrics.get("descender").copied();
    font.font_info.cap_height = master.metrics.get("capHeight").copied();
    font.font_info.x_height = master.metrics.get("xHeight").copied();
    let glyph_names: HashSet<&str> = source_glyphs.iter().map(|g| g.name.as_str()).collect();
    if glyph_names.len() != source_glyphs.len() {
        return Err("Babelfont contains duplicate glyph names".into());
    }
    let mut filenames = HashSet::new();
    let mut skipped = Vec::new();
    for source in &source_glyphs {
        Name::new(&source.name).map_err(|e| format!("glyph name: {e}"))?;
        // Babelfont uses fontTools' UFO filename convention, then appends the suffix.
        let mut filename =
            norad::user_name_to_file_name(&source.name, "", "", |_| true).into_os_string();
        filename.push(".nfsglyph");
        if !filenames.insert(filename.clone()) {
            return Err("Babelfont glyph filenames collide".into());
        }
        let layers: Vec<Layer> = read_json(&path.join("glyphs").join(filename))?;
        if layers.len() != 1
            || layers[0].master != master.id
            || layers[0].is_background
            || layers[0].location.is_some()
        {
            return Err(format!(
                "{}: only one foreground layer for the source master is supported",
                source.name
            ));
        }
        let layer = &layers[0];
        let mut glyph = Glyph::new(&source.name);
        glyph.width = layer.width;
        for cp in &source.codepoints {
            glyph.codepoints.insert(
                char::from_u32(*cp)
                    .ok_or_else(|| format!("{}: invalid Unicode value {cp}", source.name))?,
            );
        }
        for anchor in &layer.anchors {
            glyph.anchors.push(Anchor::new(
                anchor.x,
                anchor.y,
                Some(Name::new(&anchor.name).map_err(|e| e.to_string())?),
                None,
                None,
            ));
        }
        for shape in &layer.shapes {
            if let Some(reference) = &shape.reference {
                if !shape.nodes.is_empty() || !glyph_names.contains(reference.as_str()) {
                    return Err(format!("{}: invalid component {reference}", source.name));
                }
                let t = shape.transform.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                glyph.components.push(Component::new(
                    Name::new(reference).map_err(|e| e.to_string())?,
                    norad::AffineTransform {
                        x_scale: t[0],
                        xy_scale: t[1],
                        yx_scale: t[2],
                        y_scale: t[3],
                        x_offset: t[4],
                        y_offset: t[5],
                    },
                    None,
                ));
            } else {
                let mut points = shape
                    .nodes
                    .iter()
                    .map(point)
                    .collect::<Result<Vec<_>, _>>()?;
                if !shape.closed {
                    let first = points.first_mut().ok_or("empty open contour")?;
                    if first.typ == PointType::OffCurve {
                        return Err("open contour starts with an off-curve point".into());
                    }
                    first.typ = PointType::Move;
                }
                glyph.contours.push(Contour::new(points, None));
            }
        }
        // Norad validates contour topology on serialization; fail before the editor sees it.
        glyph
            .encode_xml()
            .map_err(|e| format!("{}: {e}", source.name))?;
        if !source.exported {
            skipped.push(plist::Value::String(source.name.clone()));
        }
        font.default_layer_mut().insert_glyph(glyph);
    }
    for (prefix, groups) in [
        ("public.kern1.", &info.first_kern_groups),
        ("public.kern2.", &info.second_kern_groups),
    ] {
        for (name, members) in groups {
            let members = members
                .iter()
                .map(|member| Name::new(member).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            font.groups.insert(
                Name::new(&(prefix.to_owned() + name)).map_err(|e| e.to_string())?,
                members,
            );
        }
    }
    for (pair, value) in &master.kerning {
        let (left, right) = pair
            .split_once("//")
            .ok_or_else(|| format!("invalid kerning pair {pair}"))?;
        let side = |key: &str, prefix: &str| -> Result<Name, String> {
            let name = key
                .strip_prefix('@')
                .map_or_else(|| key.to_owned(), |group| prefix.to_owned() + group);
            if !glyph_names.contains(name.as_str()) && !font.groups.contains_key(name.as_str()) {
                return Err(format!("unknown kerning glyph or group {name}"));
            }
            Name::new(&name).map_err(|e| e.to_string())
        };
        font.kerning
            .entry(side(left, "public.kern1.")?)
            .or_default()
            .insert(side(right, "public.kern2.")?, *value);
    }
    if !skipped.is_empty() {
        font.lib.insert(
            "public.skipExportGlyphs".into(),
            plist::Value::Array(skipped),
        );
    }
    let features = path.join("features.fea");
    if features.exists() {
        font.features = std::fs::read_to_string(features).map_err(|e| e.to_string())?;
        if font.features.contains("include") {
            return Err("Babelfont features with includes are not supported yet".into());
        }
    }
    Ok(font)
}

fn point(node: &Value) -> Result<ContourPoint, String> {
    let fields = node
        .as_array()
        .filter(|n| n.len() == 3 || n.len() == 4)
        .ok_or("node must be [x, y, type]")?;
    let x = fields[0].as_f64().ok_or("invalid node x")?;
    let y = fields[1].as_f64().ok_or("invalid node y")?;
    let kind = fields[2].as_str().ok_or("invalid node type")?;
    let typ = match kind {
        "l" | "ls" => PointType::Line,
        "c" | "cs" => PointType::Curve,
        "q" | "qs" => PointType::QCurve,
        "o" => PointType::OffCurve,
        _ => return Err(format!("unsupported Babelfont node type {kind}")),
    };
    Ok(ContourPoint::new(
        x,
        y,
        typ,
        kind.ends_with('s'),
        None,
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::project::Project;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/babelfont/Basic.babelfont")
    }

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn scratch() -> Scratch {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "babelfont-import-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.path().is_dir() {
                    copy(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        copy(&fixture(), &root.join("Basic.babelfont"));
        Scratch(root)
    }

    #[test]
    fn upstream_package_preserves_font_geometry_and_metrics() {
        let font = import_babelfont(&fixture()).unwrap();
        assert_eq!(
            font.font_info.family_name.as_deref(),
            Some("Babelfont Test")
        );
        assert_eq!(font.font_info.style_name.as_deref(), Some("Regular"));
        assert_eq!(font.font_info.units_per_em.unwrap().as_f64(), 1000.0);
        assert_eq!(font.font_info.ascender, Some(800.0));
        assert_eq!(font.font_info.descender, Some(-200.0));
        assert_eq!(font.default_layer().len(), 3);
        let a = font.get_glyph("A").unwrap();
        assert_eq!(a.width, 600.0);
        assert!(a.codepoints.contains('A'));
        assert_eq!(a.contours[0].points.len(), 5);
        assert_eq!(a.contours[0].points[1].typ, PointType::OffCurve);
        assert_eq!(a.contours[0].points[3].typ, PointType::Curve);
        assert!(a.contours[0].points[3].smooth);
        assert_eq!(a.anchors[0].name.as_deref(), Some("top"));
        assert_eq!(a.anchors[0].y, 700.0);
        assert_eq!(
            font.get_glyph("A.alt").unwrap().components[0]
                .transform
                .x_offset,
            25.0
        );
        assert_eq!(font.kerning.get("A").unwrap().get("V"), Some(&-80.0));
        assert_eq!(
            font.lib
                .get("public.skipExportGlyphs")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .as_string(),
            Some("A.alt")
        );
    }

    #[test]
    fn project_import_is_read_only_and_save_uses_a_new_ufo() {
        let scratch = scratch();
        let package = scratch.0.join("Basic.babelfont");
        let before = std::fs::read(package.join("glyphs/A_.nfsglyph")).unwrap();
        // An existing sibling font must not be overwritten by this import.
        let occupied = scratch.0.join("Basic.ufo");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("sentinel"), "keep").unwrap();
        let mut project = Project::load(&package).unwrap();
        let master = &mut project.masters[0];
        assert!(master.dirty);
        assert!(!master.source_path.exists());
        master.font.get_glyph_mut("A").unwrap().width = 701.0;
        master.save().unwrap();
        assert_eq!(
            Font::load(&master.source_path)
                .unwrap()
                .get_glyph("A")
                .unwrap()
                .width,
            701.0
        );
        assert_eq!(
            std::fs::read(package.join("glyphs/A_.nfsglyph")).unwrap(),
            before
        );
        assert_eq!(
            std::fs::read_to_string(occupied.join("sentinel")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn variable_sources_and_extra_layers_fail_explicitly() {
        let scratch = scratch();
        let package = scratch.0.join("Basic.babelfont");
        let info_path = package.join("info.json");
        let mut info: Value = read_json(&info_path).unwrap();
        info["axes"] = serde_json::json!([{"tag":"wght","min":100,"default":400,"max":900}]);
        std::fs::write(&info_path, info.to_string()).unwrap();
        assert!(
            import_babelfont(&package)
                .unwrap_err()
                .contains("one master")
        );
        info["axes"] = serde_json::json!([]);
        std::fs::write(&info_path, info.to_string()).unwrap();
        let layers_path = package.join("glyphs/A_.nfsglyph");
        let mut layers: Vec<Value> = read_json(&layers_path).unwrap();
        layers.push(layers[0].clone());
        std::fs::write(layers_path, serde_json::to_vec(&layers).unwrap()).unwrap();
        assert!(
            import_babelfont(&package)
                .unwrap_err()
                .contains("foreground layer")
        );
    }

    #[test]
    fn malformed_nodes_and_missing_components_are_rejected() {
        assert!(point(&serde_json::json!([1, 2, "unknown"])).is_err());
        assert!(point(&serde_json::json!(["bad", 2, "l"])).is_err());
        let scratch = scratch();
        let package = scratch.0.join("Basic.babelfont");
        let layer_path = package.join("glyphs/A_.alt.nfsglyph");
        let text = std::fs::read_to_string(&layer_path).unwrap();
        std::fs::write(&layer_path, text.replace("\"A\"", "\"missing\"")).unwrap();
        assert!(
            import_babelfont(&package)
                .unwrap_err()
                .contains("invalid component")
        );
    }
}
