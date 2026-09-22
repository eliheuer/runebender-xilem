// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read-only import of Python `.babelfont` directory packages into variable projects.
//! The package format follows simoncozens/babelfont's `convertors/nfsf.py`.
//! Import brings outlines, components, anchors, widths, Unicode, names, metrics,
//! and kerning into exact UFO payloads, including multiple sources and extra layers.
//! Saving writes new UFO/Designspace files; it never rewrites the source package.
//! Unsupported fields fail explicitly instead of being silently omitted.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use norad::{Anchor, Component, Contour, ContourPoint, Font, Glyph, Name, PointType};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Info {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    features: Value,
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
#[serde(deny_unknown_fields)]
struct SourceMaster {
    id: String,
    name: Value,
    #[serde(default)]
    metrics: BTreeMap<String, f64>,
    #[serde(default)]
    kerning: BTreeMap<String, f64>,
    #[serde(default)]
    location: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceGlyph {
    name: String,
    #[serde(default)]
    codepoints: Vec<u32>,
    #[serde(default = "yes")]
    exported: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    id: Option<String>,
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
#[serde(deny_unknown_fields)]
struct SourceAnchor {
    name: String,
    x: f64,
    y: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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

fn validate_localized(value: &Value) -> Result<(), String> {
    if value.is_string()
        || value.is_null()
        || value
            .as_object()
            .is_some_and(|map| map.len() == 1 && map.get("dflt").is_some_and(Value::is_string))
    {
        Ok(())
    } else {
        Err("localized Babelfont names beyond the default string are not supported".into())
    }
}

/// Import a Python Babelfont package as a variable project without writing its source.
/// Sources and intermediate layers receive new UFO/Designspace save destinations.
pub fn import_project(path: &Path) -> Result<crate::font::project::Project, String> {
    use crate::font::project::Project;
    use norad::designspace::{Axis, AxisMapping, DesignSpaceDocument, Dimension, Instance, Source};

    if !path.is_dir() {
        return Err("this importer expects a Python Babelfont directory package; Rust Babelfont JSON is a different format".into());
    }
    let info: Info = read_json(&path.join("info.json"))?;
    let names: BTreeMap<String, Value> = read_json(&path.join("names.json"))?;
    let glyphs: Vec<SourceGlyph> = read_json(&path.join("glyphs.json"))?;
    let mut ids = HashSet::new();
    if info.masters.is_empty() || info.masters.iter().any(|m| !ids.insert(&m.id)) {
        return Err("Babelfont needs distinct source ids".into());
    }
    let mut doc = DesignSpaceDocument {
        format: 5.0,
        ..Default::default()
    };
    for axis in &info.axes {
        let fields = axis.as_object().ok_or("axis must be an object")?;
        if fields.keys().any(|key| {
            !["name", "tag", "min", "default", "max", "map", "hidden"].contains(&key.as_str())
        }) {
            return Err("unsupported Babelfont axis field".into());
        }
        let tag = axis["tag"].as_str().ok_or("axis needs a tag")?;
        validate_localized(&axis["name"])?;
        let name = localized(&axis["name"]).unwrap_or_else(|| tag.into());
        let mut map = Vec::new();
        if let Some(pairs) = axis.get("map").filter(|v| !v.is_null()) {
            for pair in pairs.as_array().ok_or("axis map must be an array")? {
                if pair.as_array().is_none_or(|pair| pair.len() != 2) {
                    return Err("axis map entries must be [user, design] pairs".into());
                }
                map.push(AxisMapping {
                    input: coordinate(&pair[0])?,
                    output: coordinate(&pair[1])?,
                });
            }
        }
        doc.axes.push(Axis {
            name,
            tag: tag.into(),
            minimum: Some(coordinate(&axis["min"])?),
            default: coordinate(&axis["default"])?,
            maximum: Some(coordinate(&axis["max"])?),
            map: (!map.is_empty()).then_some(map),
            hidden: axis["hidden"].as_bool().unwrap_or(false),
            ..Default::default()
        });
    }
    let dimensions = |location: &BTreeMap<String, f64>| -> Result<Vec<Dimension>, String> {
        if location
            .keys()
            .any(|tag| !doc.axes.iter().any(|a| &a.tag == tag))
        {
            return Err("source location references an unknown axis".into());
        }
        doc.axes
            .iter()
            .filter_map(|axis| location.get(&axis.tag).map(|value| (axis, value)))
            .map(|(axis, value)| {
                Ok(Dimension {
                    name: axis.name.clone(),
                    xvalue: Some(coordinate(&Value::from(*value))?),
                    ..Default::default()
                })
            })
            .collect()
    };
    let mut sources = Vec::new();
    for (index, master) in info.masters.iter().enumerate() {
        let (font, intermediates) = import_master(path, &info, master, &names, &glyphs)?;
        let filename = format!("source-{index}.ufo");
        doc.sources.push(Source {
            filename: filename.clone(),
            name: Some(master.id.clone()),
            stylename: localized(&master.name),
            location: dimensions(&master.location)?,
            ..Default::default()
        });
        for (layer, location) in intermediates {
            let location = doc
                .axes
                .iter()
                .zip(location)
                .map(|(axis, value)| (axis.tag.clone(), value))
                .collect();
            doc.sources.push(Source {
                filename: filename.clone(),
                layer: Some(layer),
                location: dimensions(&location)?,
                ..Default::default()
            });
        }
        sources.push(font);
    }
    for instance in &info.instances {
        validate_localized(&instance["name"])?;
        let fields = instance.as_object().ok_or("instance must be an object")?;
        if fields
            .keys()
            .any(|key| !["name", "location", "variable"].contains(&key.as_str()))
        {
            return Err("unsupported Babelfont instance metadata".into());
        }
        if instance["variable"].as_bool() == Some(true) {
            return Err("variable named-instance ranges are not supported".into());
        }
        let location = serde_json::from_value(instance["location"].clone())
            .map_err(|e| format!("instance location: {e}"))?;
        doc.instances.push(Instance {
            name: localized(&instance["name"]),
            stylename: localized(&instance["name"]),
            location: dimensions(&location)?,
            ..Default::default()
        });
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let single = sources.len() == 1
        && doc.axes.is_empty()
        && doc.sources.len() == 1
        && doc.instances.is_empty();
    let mut destination = if single {
        path.with_extension("ufo")
    } else {
        path.with_file_name(format!("{stem}-import"))
    };
    let mut index = 1;
    while destination.exists() {
        destination = path.with_file_name(if single {
            format!("{stem}-import-{index}.ufo")
        } else {
            format!("{stem}-import-{index}")
        });
        index += 1;
    }
    let mut project = if single {
        Project::from_imported_ufo_boundary(destination.clone(), &sources.remove(0))?
    } else {
        let sources = sources
            .into_iter()
            .enumerate()
            .map(|(index, font)| {
                let filename = format!("source-{index}.ufo");
                let source_path = destination.join(&filename);
                (filename, font, source_path)
            })
            .collect();
        Project::from_imported_designspace_boundary(doc, sources)?
    };
    project.export_source = Some(if single {
        destination
    } else {
        destination.join("font.designspace")
    });
    project.ds_dirty = !single;
    Ok(project)
}

fn coordinate(value: &Value) -> Result<f32, String> {
    let number = value.as_f64().ok_or("coordinate must be a number")?;
    let narrowed: f32 = number
        .to_string()
        .parse()
        .map_err(|_| "coordinate cannot be represented")?;
    if !number.is_finite() || narrowed.to_string().parse::<f64>().ok() != Some(number) {
        return Err("coordinate cannot round-trip through the Designspace adapter".into());
    }
    Ok(narrowed)
}

/// Read a single-master Babelfont package as an editable UFO, without writing files.
///
/// Requires `info.json`, `names.json`, `glyphs.json`, and the indexed `.nfsglyph`
/// files. Rejects multiple masters, axes, instances, additional/background layers,
/// invalid nodes, missing components, and malformed metadata. Features containing
/// external includes are rejected because their paths cannot survive UFO import.
/// Unsupported guides, hints, production names and application metadata are rejected.
pub fn import_babelfont(path: &Path) -> Result<Font, String> {
    let info: Info = read_json(&path.join("info.json"))?;
    if info.masters.len() != 1 || !info.axes.is_empty() || !info.instances.is_empty() {
        return Err(
            "Babelfont import currently supports one master without axes or instances".into(),
        );
    }
    let names: BTreeMap<String, Value> = read_json(&path.join("names.json"))?;
    let source_glyphs: Vec<SourceGlyph> = read_json(&path.join("glyphs.json"))?;
    let (font, _) = import_master(path, &info, &info.masters[0], &names, &source_glyphs)?;
    if font.layers.len() != 1 {
        return Err("only one foreground layer is supported by the single-font adapter".into());
    }
    Ok(font)
}

fn import_master(
    path: &Path,
    info: &Info,
    master: &SourceMaster,
    names: &BTreeMap<String, Value>,
    source_glyphs: &[SourceGlyph],
) -> Result<(Font, BTreeMap<String, Vec<f64>>), String> {
    if !info.upm.is_finite() || info.upm <= 0.0 {
        return Err("Babelfont units per em must be positive and finite".into());
    }
    let mut intermediates = BTreeMap::new();
    if !info.features.is_null()
        && info
            .features
            .as_object()
            .is_none_or(|value| !value.is_empty())
    {
        return Err("Babelfont feature objects are unsupported; use features.fea".into());
    }
    if master
        .metrics
        .keys()
        .any(|name| !["ascender", "descender", "capHeight", "xHeight"].contains(&name.as_str()))
    {
        return Err("unsupported Babelfont source metric".into());
    }
    let mut font = Font::default();
    validate_localized(&master.name)?;
    for (name, value) in names {
        validate_localized(value)?;
        let target = match name.as_str() {
            "familyName" => &mut font.font_info.family_name,
            "copyright" => &mut font.font_info.copyright,
            "trademark" => &mut font.font_info.trademark,
            "designer" => &mut font.font_info.open_type_name_designer,
            "designerURL" => &mut font.font_info.open_type_name_designer_url,
            "manufacturer" => &mut font.font_info.open_type_name_manufacturer,
            "manufacturerURL" => &mut font.font_info.open_type_name_manufacturer_url,
            "license" => &mut font.font_info.open_type_name_license,
            "licenseURL" => &mut font.font_info.open_type_name_license_url,
            "description" => &mut font.font_info.open_type_name_description,
            _ => return Err(format!("unsupported Babelfont name field {name}")),
        };
        *target = localized(value);
    }
    font.font_info.open_type_head_created = info.date.as_ref().map(|date| date.replace('-', "/"));
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
    for source in source_glyphs {
        Name::new(&source.name).map_err(|e| format!("glyph name: {e}"))?;
        // Babelfont uses fontTools' UFO filename convention, then appends the suffix.
        let mut filename =
            norad::user_name_to_file_name(&source.name, "", "", |_| true).into_os_string();
        filename.push(".nfsglyph");
        if !filenames.insert(filename.clone()) {
            return Err("Babelfont glyph filenames collide".into());
        }
        let layers: Vec<Layer> = read_json(&path.join("glyphs").join(filename))?;
        if layers
            .iter()
            .any(|layer| !info.masters.iter().any(|m| m.id == layer.master))
        {
            return Err(format!("{}: unknown layer master", source.name));
        }
        let mut default_seen = false;
        let mut named_seen = HashSet::new();
        for layer in layers.iter().filter(|layer| layer.master == master.id) {
            let is_default =
                !layer.is_background && layer.location.is_none() && layer.name.is_none();
            let layer_name = if is_default {
                if default_seen {
                    return Err(format!("{}: multiple foreground layers", source.name));
                }
                default_seen = true;
                "public.default".to_string()
            } else {
                layer
                    .name
                    .clone()
                    .or_else(|| layer.is_background.then(|| "public.background".to_string()))
                    .or_else(|| layer.id.clone())
                    .ok_or("additional Babelfont layer needs a name or id")?
            };
            if !named_seen.insert(layer_name.clone()) {
                return Err(format!("{}: duplicate layer {layer_name}", source.name));
            }
            if let Some(location) = &layer.location {
                if location.len() != info.axes.len() || location.iter().any(|v| !v.is_finite()) {
                    return Err(format!(
                        "{}: invalid intermediate layer location",
                        source.name
                    ));
                }
                if let Some(previous) = intermediates.insert(layer_name.clone(), location.clone())
                    && previous != *location
                {
                    return Err(format!(
                        "{layer_name}: conflicting glyph-specific locations"
                    ));
                }
            }
            let mut glyph = Glyph::new(&source.name);
            super::metadata::lib_keys::write_babelfont_layer(
                &mut glyph,
                &layer.master,
                layer.id.as_deref(),
                layer.is_background,
            );
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
                    if shape.transform.is_some() {
                        return Err("Babelfont contour transforms are unsupported".into());
                    }
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
            if !source.exported && is_default {
                skipped.push(source.name.clone());
            }
            font.layers
                .get_or_create_layer(&layer_name)
                .map_err(|e| e.to_string())?
                .insert_glyph(glyph);
        }
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
    crate::font::model::glyph_metadata::set_skipped_exports(&mut font, skipped);
    let features = path.join("features.fea");
    if features.exists() {
        font.features = std::fs::read_to_string(features).map_err(|e| e.to_string())?;
        if font.features.contains("include") {
            return Err("Babelfont features with includes are not supported yet".into());
        }
    }
    Ok((font, intermediates))
}

fn point(node: &Value) -> Result<ContourPoint, String> {
    let fields = node
        .as_array()
        .filter(|n| n.len() == 3)
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
    use crate::font::project::Project;
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
            crate::font::model::glyph_metadata::skipped_exports(&font).next(),
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
        let source = project.source_id(0).unwrap();
        let source_path = project.document_source_path(source).unwrap().to_owned();
        assert_eq!(project.document_source_is_modified(source), Some(true));
        assert!(!source_path.exists());
        let layer = project.document_source(source).unwrap().default_layer();
        assert!(matches!(
            project.edit_document_layer("A", &layer, |draft| {
                draft.set_width(701.0)?;
                Ok(())
            }),
            Ok(crate::font::project::DocumentEditOutcome::Changed { .. })
        ));
        project.save().unwrap();
        assert_eq!(
            Font::load(&source_path)
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
    fn variable_package_preserves_sources_layers_and_original_files() {
        let scratch = scratch();
        let package = scratch.0.join("Basic.babelfont");
        let info_path = package.join("info.json");
        let mut info: Value = read_json(&info_path).unwrap();
        info["axes"] = serde_json::json!([{
            "name": "Weight", "tag": "wght", "min": 100, "default": 100,
            "max": 900, "map": [[100, 0], [900, 100]]
        }]);
        info["masters"][0]["location"] = serde_json::json!({"wght": 0});
        let mut heavy = info["masters"][0].clone();
        heavy["id"] = "M2".into();
        heavy["name"] = "Heavy".into();
        heavy["location"] = serde_json::json!({"wght": 100});
        info["masters"].as_array_mut().unwrap().push(heavy);
        info["instances"] = serde_json::json!([{
            "name": "Medium", "location": {"wght": 50}
        }]);
        std::fs::write(&info_path, info.to_string()).unwrap();
        let mut originals = BTreeMap::new();
        for file in std::fs::read_dir(package.join("glyphs")).unwrap() {
            let path = file.unwrap().path();
            let mut layers: Vec<Value> = read_json(&path).unwrap();
            let mut heavy = layers[0].clone();
            heavy["_master"] = "M2".into();
            heavy["id"] = "heavy-layer".into();
            heavy["width"] = 800.123_456_789.into();
            layers.push(heavy);
            if path.file_name().unwrap() == "A_.nfsglyph" {
                let mut intermediate = layers[0].clone();
                intermediate["name"] = "intermediate".into();
                intermediate["location"] = serde_json::json!([50]);
                intermediate["width"] = 731.123_456_789.into();
                layers.push(intermediate);
                let mut background = layers[0].clone();
                background["isBackground"] = true.into();
                layers.push(background);
            }
            let bytes = serde_json::to_vec(&layers).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            originals.insert(path, bytes);
        }
        originals.insert(info_path.clone(), std::fs::read(info_path).unwrap());
        let mut project = Project::load(&package).unwrap();
        assert_eq!(project.document_sources().count(), 2);
        assert_eq!(project.instances[0].1["Weight"], 0.5);
        assert_eq!(
            project
                .try_encode_interpolated_ufo_at("A", &[("Weight".into(), 0.5)].into())
                .unwrap()
                .width,
            731.123_456_789
        );
        let layer = crate::font::variable::LayerId {
            source: crate::font::variable::SourceId(0),
            name: "public.background".into(),
        };
        assert_eq!(
            super::super::metadata::lib_keys::read_babelfont_layer(
                &project.encode_ufo_layer("A", &layer).unwrap()
            ),
            Some(("M1", Some("A-M1"), true))
        );
        let destination = project.export_source.clone().unwrap();
        assert!(!destination.exists());
        project.save().unwrap();
        let reloaded = Project::load(&destination).unwrap();
        assert_eq!(reloaded.document_sources().count(), 2);
        assert_eq!(reloaded.variable_glyph("A").unwrap().layer_ids().count(), 4);
        assert_eq!(
            reloaded
                .try_encode_interpolated_ufo_at("A", &[("Weight".into(), 0.5)].into())
                .unwrap()
                .width,
            731.123_456_789
        );
        for (path, bytes) in originals {
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }

    #[test]
    fn unsupported_package_metadata_is_an_error() {
        let scratch = scratch();
        let package = scratch.0.join("Basic.babelfont");
        let path = package.join("names.json");
        let names = std::fs::read(&path).unwrap();
        std::fs::write(
            &path,
            br#"{"familyName":{"dflt":"Example","fr":"Exemple"}}"#,
        )
        .unwrap();
        assert!(import_project(&package).unwrap_err().contains("localized"));
        std::fs::write(&path, names).unwrap();
        let path = package.join("glyphs/A_.nfsglyph");
        let mut layers: Vec<Value> = read_json(&path).unwrap();
        layers[0]["guides"] = serde_json::json!([{"x": 10}]);
        std::fs::write(&path, serde_json::to_vec(&layers).unwrap()).unwrap();
        assert!(import_project(&package).unwrap_err().contains("guides"));
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
