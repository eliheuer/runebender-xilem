//! Import Glyphs sources (.glyphs and .glyphspackage, Glyphs 2 or 3)
//! by converting them to an in-memory UFO + designspace file set.
//!
//! Stage 1 of Glyphs support: the editor's existing UFO/designspace
//! load pipeline stays the single ingestion path; a .glyphs file is
//! translated into the same shape (per-master UFOs and, for
//! multi-master fonts, a designspace) before it ever reaches the
//! loader. Read-only: nothing here writes .glyphs back.
//!
//! Scope: outlines, components (position/rotation/scale), anchors,
//! widths, unicodes, per-master vertical metrics, kerning + kerning
//! groups. Skipped for now: brace/bracket layers, smart components,
//! OpenType features, color labels, hints. Conversion choices follow
//! glyphsLib where a convention exists (closed-contour node rotation,
//! kern-group naming, italic-angle sign).

use std::collections::{BTreeMap, HashSet};

use glyphslib::Font as GlyphsFont;
use glyphslib::common::NodeType;
use glyphslib::glyphs3::{Glyphs3, MetricType, Shape};
use serde::Serialize;

#[derive(Serialize)]
pub struct ConvertedFile {
    pub path: String,
    pub text: String,
}

#[derive(Serialize)]
pub struct ConversionResult {
    pub family_name: String,
    pub files: Vec<ConvertedFile>,
    pub warnings: Vec<String>,
}

/// Convert a single-file `.glyphs` source.
pub fn glyphs_to_ufo_files(glyphs_text: &str) -> Result<ConversionResult, String> {
    let font = GlyphsFont::load_str(glyphs_text).map_err(|e| format!("parse .glyphs: {e}"))?;
    convert(font)
}

/// Convert a `.glyphspackage` from its files, keyed by path relative to
/// the package root: `fontinfo.plist`, `order.plist`, the optional
/// `UIState.plist`, and `glyphs/<name>.glyph`.
///
/// Taking the entries rather than a directory keeps this usable where
/// there is no filesystem to read, which is what the browser build
/// needs.
pub fn glyphs_package_to_ufo_files(
    entries: &std::collections::HashMap<String, String>,
) -> Result<ConversionResult, String> {
    if !entries.keys().any(|k| k.ends_with("fontinfo.plist")) {
        return Err("no fontinfo.plist in .glyphspackage".into());
    }
    let font = GlyphsFont::load_package_entries(entries)
        .map_err(|e| format!("parse .glyphspackage: {e}"))?;
    convert(font)
}

fn convert(font: GlyphsFont) -> Result<ConversionResult, String> {
    let font = font.upgrade();
    let font: &Glyphs3 = match &font {
        GlyphsFont::Glyphs3(f) => f,
        GlyphsFont::Glyphs2(_) => return Err("internal: upgrade did not yield Glyphs 3".into()),
    };
    if font.masters.is_empty() {
        return Err("no masters in the Glyphs source".into());
    }

    let mut files = Vec::new();
    let mut warnings = Vec::new();
    let family = font.family_name.trim();
    let family = if family.is_empty() {
        "Untitled"
    } else {
        family
    };
    let family_compact: String = family.chars().filter(|c| !c.is_whitespace()).collect();

    for master in &font.masters {
        let master_name = master_display_name(font, master);
        let ufo_dir = format!("{family_compact}-{}.ufo", compact(&master_name));

        // metainfo + layercontents make the synthesized UFO honest.
        files.push(ConvertedFile {
            path: format!("{ufo_dir}/metainfo.plist"),
            text: plist_doc(
                "<dict>\n  <key>creator</key>\n  <string>org.runebender.glyphs-import</string>\n  <key>formatVersion</key>\n  <integer>3</integer>\n</dict>",
            ),
        });
        files.push(ConvertedFile {
            path: format!("{ufo_dir}/layercontents.plist"),
            text: plist_doc(
                "<array>\n  <array>\n    <string>public.default</string>\n    <string>glyphs</string>\n  </array>\n</array>",
            ),
        });

        files.push(ConvertedFile {
            path: format!("{ufo_dir}/fontinfo.plist"),
            text: fontinfo_plist(font, master, family, &master_name),
        });

        // Glyphs for this master's main layer.
        let mut existing = HashSet::new();
        let mut contents = String::new();
        for glyph in &font.glyphs {
            let Some(layer) = glyph
                .layers
                .iter()
                .find(|l| l.layer_id == master.id && l.associated_master_id.is_none())
                .or_else(|| glyph.layers.iter().find(|l| l.layer_id == master.id))
            else {
                continue;
            };
            match layer_to_glif(font, glyph, layer) {
                Ok(xml) => {
                    let file_name =
                        norad::user_name_to_file_name(&glyph.name, "", ".glif", |candidate| {
                            !existing.contains(candidate)
                        });
                    let file_name = file_name.to_string_lossy().to_string();
                    existing.insert(file_name.clone());
                    contents.push_str(&format!(
                        "  <key>{}</key>\n  <string>{}</string>\n",
                        xml_escape(&glyph.name),
                        xml_escape(&file_name)
                    ));
                    files.push(ConvertedFile {
                        path: format!("{ufo_dir}/glyphs/{file_name}"),
                        text: xml,
                    });
                }
                Err(e) => warnings.push(format!("{} ({master_name}): {e}", glyph.name)),
            }
        }
        files.push(ConvertedFile {
            path: format!("{ufo_dir}/glyphs/contents.plist"),
            text: plist_doc(&format!("<dict>\n{contents}</dict>")),
        });

        // Kerning groups + per-master kerning (LTR only for now).
        files.push(ConvertedFile {
            path: format!("{ufo_dir}/groups.plist"),
            text: groups_plist(font),
        });
        if let Some(kerning) = font.kerning.get(&master.id) {
            files.push(ConvertedFile {
                path: format!("{ufo_dir}/kerning.plist"),
                text: kerning_plist(kerning),
            });
        }
    }

    if font.masters.len() > 1 {
        files.push(ConvertedFile {
            path: format!("{family_compact}.designspace"),
            text: designspace_xml(font, family, &family_compact),
        });
    }

    Ok(ConversionResult {
        family_name: family.to_string(),
        files,
        warnings,
    })
}

fn compact(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).collect()
}

fn master_display_name(font: &Glyphs3, master: &glyphslib::glyphs3::Master) -> String {
    if !master.name.trim().is_empty() {
        return master.name.trim().to_string();
    }
    // Fall back to a location-derived label so multi-master fonts
    // without names still get distinct UFO directories.
    let index = font
        .masters
        .iter()
        .position(|m| m.id == master.id)
        .unwrap_or(0);
    if master.axes_values.is_empty() {
        format!("Master{index}")
    } else {
        master
            .axes_values
            .iter()
            .map(|v| fmt_f32(*v))
            .collect::<Vec<_>>()
            .join("-")
    }
}

fn metric_value(
    font: &Glyphs3,
    master: &glyphslib::glyphs3::Master,
    wanted: MetricType,
) -> Option<f32> {
    font.metrics
        .iter()
        .position(|m| m.metric_type.as_ref() == Some(&wanted))
        .and_then(|i| master.metric_values.get(i))
        .map(|v| v.pos)
}

fn fontinfo_plist(
    font: &Glyphs3,
    master: &glyphslib::glyphs3::Master,
    family: &str,
    master_name: &str,
) -> String {
    let mut body = String::from("<dict>\n");
    let mut push_str = |key: &str, value: &str| {
        body.push_str(&format!(
            "  <key>{key}</key>\n  <string>{}</string>\n",
            xml_escape(value)
        ));
    };
    push_str("familyName", family);
    push_str("styleName", master_name);
    let push_num = |body: &mut String, key: &str, value: f64| {
        body.push_str(&format!(
            "  <key>{key}</key>\n  <real>{}</real>\n",
            fmt_f64(value)
        ));
    };
    push_num(&mut body, "unitsPerEm", font.units_per_em as f64);
    let upm = font.units_per_em as f32;
    let ascender = metric_value(font, master, MetricType::Ascender).unwrap_or(upm * 0.8);
    let descender = metric_value(font, master, MetricType::Descender).unwrap_or(-(upm * 0.2));
    push_num(&mut body, "ascender", ascender as f64);
    push_num(&mut body, "descender", descender as f64);
    if let Some(v) = metric_value(font, master, MetricType::XHeight) {
        push_num(&mut body, "xHeight", v as f64);
    }
    if let Some(v) = metric_value(font, master, MetricType::CapHeight) {
        push_num(&mut body, "capHeight", v as f64);
    }
    if let Some(v) = metric_value(font, master, MetricType::ItalicAngle) {
        if v != 0.0 {
            // Glyphs stores the slant clockwise; UFO italicAngle is
            // counter-clockwise (glyphsLib negates the same way).
            push_num(&mut body, "italicAngle", -v as f64);
        }
    }
    body.push_str("</dict>");
    plist_doc(&body)
}

fn layer_to_glif(
    font: &Glyphs3,
    glyph: &glyphslib::glyphs3::Glyph,
    layer: &glyphslib::glyphs3::Layer,
) -> Result<String, String> {
    let mut out = norad::Glyph::new(&glyph.name);
    out.width = layer.width as f64;
    out.codepoints = glyph
        .unicode
        .iter()
        .filter_map(|cp| char::from_u32(*cp))
        .collect();

    for anchor in &layer.anchors {
        out.anchors.push(norad::Anchor::new(
            anchor.pos.0 as f64,
            anchor.pos.1 as f64,
            Some(
                norad::Name::new(&anchor.name)
                    .map_err(|e| format!("anchor name {:?}: {e}", anchor.name))?,
            ),
            None,
            None,
        ));
    }

    for shape in &layer.shapes {
        match shape {
            Shape::Path(path) => {
                let mut points = Vec::new();
                let mut nodes: Vec<&glyphslib::glyphs3::Node> = path.nodes.iter().collect();
                if nodes.is_empty() {
                    continue;
                }
                if path.closed {
                    // Glyphs stores a closed contour's start node at the
                    // END of the node list; UFO wants it first.
                    nodes.rotate_right(1);
                } else {
                    let first = nodes.remove(0);
                    points.push(norad::ContourPoint::new(
                        first.x as f64,
                        first.y as f64,
                        norad::PointType::Move,
                        is_smooth(first.node_type),
                        None,
                        None,
                    ));
                }
                for node in nodes {
                    points.push(norad::ContourPoint::new(
                        node.x as f64,
                        node.y as f64,
                        point_type(node.node_type),
                        is_smooth(node.node_type),
                        None,
                        None,
                    ));
                }
                out.contours.push(norad::Contour::new(points, None));
            }
            Shape::Component(component) => {
                // glyphsLib composition order: translate, rotate, scale,
                // then slant. Most sources only use position and scale.
                let mut affine = kurbo::Affine::translate((
                    component.position.0 as f64,
                    component.position.1 as f64,
                ));
                if component.angle != 0.0 {
                    affine *= kurbo::Affine::rotate((component.angle as f64).to_radians());
                }
                affine *= kurbo::Affine::scale_non_uniform(
                    component.scale.0 as f64,
                    component.scale.1 as f64,
                );
                if component.slant.0 != 0.0 || component.slant.1 != 0.0 {
                    affine *= kurbo::Affine::skew(
                        (component.slant.0 as f64).to_radians().tan(),
                        (component.slant.1 as f64).to_radians().tan(),
                    );
                }
                let c = affine.as_coeffs();
                if !font
                    .glyphs
                    .iter()
                    .any(|g| g.name == component.component_glyph)
                {
                    return Err(format!(
                        "component references missing glyph {:?}",
                        component.component_glyph
                    ));
                }
                out.components.push(norad::Component::new(
                    norad::Name::new(&component.component_glyph)
                        .map_err(|e| format!("component name: {e}"))?,
                    norad::AffineTransform {
                        x_scale: c[0],
                        xy_scale: c[1],
                        yx_scale: c[2],
                        y_scale: c[3],
                        x_offset: c[4],
                        y_offset: c[5],
                    },
                    None,
                ));
            }
        }
    }

    let bytes = out.encode_xml().map_err(|e| format!("encode .glif: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("glif not utf-8: {e}"))
}

fn point_type(node_type: NodeType) -> norad::PointType {
    match node_type {
        NodeType::Line | NodeType::LineSmooth => norad::PointType::Line,
        NodeType::Curve | NodeType::CurveSmooth => norad::PointType::Curve,
        NodeType::QCurve | NodeType::QCurveSmooth => norad::PointType::QCurve,
        NodeType::OffCurve => norad::PointType::OffCurve,
    }
}

fn is_smooth(node_type: NodeType) -> bool {
    matches!(
        node_type,
        NodeType::LineSmooth | NodeType::CurveSmooth | NodeType::QCurveSmooth
    )
}

/// Kerning class names: Glyphs `@MMK_L_x` / `@MMK_R_x` become UFO
/// `public.kern1.x` / `public.kern2.x`; plain names pass through.
fn kern_name(raw: &str, first_side: bool) -> String {
    if let Some(rest) = raw.strip_prefix("@MMK_L_") {
        return format!("public.kern1.{rest}");
    }
    if let Some(rest) = raw.strip_prefix("@MMK_R_") {
        return format!("public.kern2.{rest}");
    }
    if let Some(rest) = raw.strip_prefix('@') {
        // Bare class reference; side decides the UFO prefix.
        return if first_side {
            format!("public.kern1.{rest}")
        } else {
            format!("public.kern2.{rest}")
        };
    }
    raw.to_string()
}

fn groups_plist(font: &Glyphs3) -> String {
    let mut groups: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for glyph in &font.glyphs {
        // A glyph's RIGHT group forms the FIRST member of kern pairs
        // (public.kern1), its LEFT group the second — same as glyphsLib.
        if let Some(group) = &glyph.kern_right {
            groups
                .entry(format!("public.kern1.{group}"))
                .or_default()
                .push(&glyph.name);
        }
        if let Some(group) = &glyph.kern_left {
            groups
                .entry(format!("public.kern2.{group}"))
                .or_default()
                .push(&glyph.name);
        }
    }
    let mut body = String::from("<dict>\n");
    for (group, members) in groups {
        body.push_str(&format!("  <key>{}</key>\n  <array>\n", xml_escape(&group)));
        for member in members {
            body.push_str(&format!("    <string>{}</string>\n", xml_escape(member)));
        }
        body.push_str("  </array>\n");
    }
    body.push_str("</dict>");
    plist_doc(&body)
}

fn kerning_plist(kerning: &BTreeMap<String, BTreeMap<String, f32>>) -> String {
    let mut body = String::from("<dict>\n");
    for (first, pairs) in kerning {
        body.push_str(&format!(
            "  <key>{}</key>\n  <dict>\n",
            xml_escape(&kern_name(first, true))
        ));
        for (second, value) in pairs {
            body.push_str(&format!(
                "    <key>{}</key>\n    <real>{}</real>\n",
                xml_escape(&kern_name(second, false)),
                fmt_f32(*value)
            ));
        }
        body.push_str("  </dict>\n");
    }
    body.push_str("</dict>");
    plist_doc(&body)
}

fn designspace_xml(font: &Glyphs3, family: &str, family_compact: &str) -> String {
    // Default master: "Variable Font Origin" custom parameter when
    // present, else the first master.
    let origin_id = font
        .custom_parameters
        .iter()
        .find(|p| p.name == "Variable Font Origin" || p.name == "Variation Font Origin")
        .and_then(|p| p.value.as_str())
        .unwrap_or(&font.masters[0].id);
    let default_master = font
        .masters
        .iter()
        .find(|m| m.id == origin_id)
        .unwrap_or(&font.masters[0]);

    let mut xml = String::from(
        "<?xml version='1.0' encoding='UTF-8'?>\n<designspace format=\"4.1\">\n  <axes>\n",
    );
    for (i, axis) in font.axes.iter().enumerate() {
        let values: Vec<f32> = font
            .masters
            .iter()
            .map(|m| m.axes_values.get(i).copied().unwrap_or(0.0))
            .collect();
        let min = values.iter().copied().fold(f32::INFINITY, f32::min);
        let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let default = default_master.axes_values.get(i).copied().unwrap_or(min);
        xml.push_str(&format!(
            "    <axis tag=\"{}\" name=\"{}\" minimum=\"{}\" maximum=\"{}\" default=\"{}\"/>\n",
            xml_escape(&axis.tag),
            xml_escape(&axis.name),
            fmt_f32(min),
            fmt_f32(max),
            fmt_f32(default)
        ));
    }
    xml.push_str("  </axes>\n  <sources>\n");
    for master in &font.masters {
        let master_name = master_display_name(font, master);
        xml.push_str(&format!(
            "    <source filename=\"{}-{}.ufo\" familyname=\"{}\" stylename=\"{}\">\n      <location>\n",
            xml_escape(family_compact),
            xml_escape(&compact(&master_name)),
            xml_escape(family),
            xml_escape(&master_name)
        ));
        for (i, axis) in font.axes.iter().enumerate() {
            xml.push_str(&format!(
                "        <dimension name=\"{}\" xvalue=\"{}\"/>\n",
                xml_escape(&axis.name),
                fmt_f32(master.axes_values.get(i).copied().unwrap_or(0.0))
            ));
        }
        xml.push_str("      </location>\n    </source>\n");
    }
    xml.push_str("  </sources>\n</designspace>\n");
    xml
}

fn plist_doc(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n{body}\n</plist>\n"
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fmt_f32(v: f32) -> String {
    fmt_f64(v as f64)
}

fn fmt_f64(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write the sample font out as a .glyphspackage, read the files
    /// back as entries, and convert. A package holds the same data
    /// split across files, so it must land on the same UFO set the
    /// single-file path produces.
    #[test]
    fn package_matches_single_file() {
        let from_file = glyphs_to_ufo_files(MINIMAL_GLYPHS3).unwrap();

        let dir = std::env::temp_dir()
            .join(format!("rb-glyphspackage-{}", std::process::id()));
        let pkg = dir.join("TestSans.glyphspackage");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        GlyphsFont::load_str(MINIMAL_GLYPHS3)
            .unwrap()
            .save(&pkg)
            .unwrap();

        let mut entries = std::collections::HashMap::new();
        for entry in walk(&pkg) {
            let rel = entry
                .strip_prefix(&pkg)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            entries.insert(rel, std::fs::read_to_string(&entry).unwrap());
        }
        assert!(entries.contains_key("fontinfo.plist"));
        assert!(entries.contains_key("order.plist"));
        assert!(entries.keys().any(|k| k.starts_with("glyphs/")));

        let from_package = glyphs_package_to_ufo_files(&entries).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(from_package.family_name, from_file.family_name);
        let mut a: Vec<&str> =
            from_package.files.iter().map(|f| f.path.as_str()).collect();
        let mut b: Vec<&str> =
            from_file.files.iter().map(|f| f.path.as_str()).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }

    #[test]
    fn package_without_fontinfo_is_rejected() {
        let entries = std::collections::HashMap::new();
        let Err(err) = glyphs_package_to_ufo_files(&entries) else {
            panic!("an empty package should not convert");
        };
        assert!(err.contains("fontinfo.plist"), "{err}");
    }

    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(read) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    const MINIMAL_GLYPHS3: &str = r#"{
.formatVersion = 3;
familyName = "Test Sans";
unitsPerEm = 1000;
metrics = ( { type = ascender; }, { type = descender; }, { type = "x-height"; } );
fontMaster = (
  { id = "m01"; name = Regular; axesValues = ( 400 ); metricValues = ( { pos = 800; }, { pos = -200; }, { pos = 500; } ); },
  { id = "m02"; name = Bold; axesValues = ( 700 ); metricValues = ( { pos = 810; }, { pos = -190; }, { pos = 520; } ); }
);
axes = ( { name = Weight; tag = wght; } );
glyphs = (
  {
    glyphname = A;
    unicode = 65;
    kernRight = A;
    layers = (
      { layerId = "m01"; width = 600; shapes = ( { closed = 1; nodes = ( (0,0,l), (300,700,l), (600,0,l) ); } ); anchors = ( { name = top; pos = (300,700); } ); },
      { layerId = "m02"; width = 620; shapes = ( { closed = 1; nodes = ( (0,0,l), (310,700,l), (620,0,l) ); } ); }
    );
  },
  {
    glyphname = Aacute;
    unicode = 193;
    layers = (
      { layerId = "m01"; width = 600; shapes = ( { ref = A; } ); },
      { layerId = "m02"; width = 620; shapes = ( { ref = A; } ); }
    );
  }
);
kerningLTR = { m01 = { "@MMK_L_A" = { "@MMK_R_A" = -20; }; }; };
}"#;

    #[test]
    fn converts_minimal_glyphs3() {
        let result = glyphs_to_ufo_files(MINIMAL_GLYPHS3).unwrap();
        assert_eq!(result.family_name, "Test Sans");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"TestSans.designspace"));
        assert!(paths.contains(&"TestSans-Regular.ufo/fontinfo.plist"));
        assert!(paths.contains(&"TestSans-Bold.ufo/glyphs/A_.glif"));

        let glif = result
            .files
            .iter()
            .find(|f| f.path == "TestSans-Regular.ufo/glyphs/A_.glif")
            .unwrap();
        assert!(glif.text.contains("unicode hex=\"0041\""));
        // Closed contour: Glyphs' trailing start node becomes the
        // first UFO point.
        assert!(
            glif.text
                .contains("<point x=\"600\" y=\"0\" type=\"line\"/>")
        );
        assert!(glif.text.contains("anchor"));

        let aacute = result
            .files
            .iter()
            .find(|f| f.path == "TestSans-Regular.ufo/glyphs/A_acute.glif")
            .unwrap();
        assert!(aacute.text.contains("component base=\"A\""));

        let kerning = result
            .files
            .iter()
            .find(|f| f.path == "TestSans-Regular.ufo/kerning.plist")
            .unwrap();
        assert!(kerning.text.contains("public.kern1.A"));
        assert!(kerning.text.contains("public.kern2.A"));
        assert!(kerning.text.contains("-20"));

        let groups = result
            .files
            .iter()
            .find(|f| f.path == "TestSans-Regular.ufo/groups.plist")
            .unwrap();
        assert!(groups.text.contains("public.kern1.A"));

        let ds = result
            .files
            .iter()
            .find(|f| f.path == "TestSans.designspace")
            .unwrap();
        assert!(ds.text.contains("tag=\"wght\""));
        assert!(
            ds.text
                .contains("minimum=\"400\" maximum=\"700\" default=\"400\"")
        );
    }
}
