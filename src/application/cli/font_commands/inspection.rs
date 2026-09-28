// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Headless source inspection, proof generation, and MCP proof content.

use super::*;

/// What a font is, for a person or a program about to work on it.
pub(in crate::application::cli) fn info(source: &Path, list_glyphs: bool, json: bool) -> i32 {
    let project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let default_layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let glyph_names = project
        .glyph_names()
        .filter(|name| project.document_layer(name, &default_layer).is_some())
        .collect::<Vec<_>>();
    let drawn = glyph_names
        .iter()
        .filter(|name| {
            project
                .document_layer(name, &default_layer)
                .is_some_and(|layer| {
                    layer.contours().next().is_some() || layer.components().next().is_some()
                })
        })
        .count();
    let proposals = proposal::list_project(&project, source_id);
    let layers = project
        .document_source_layer_names(source_id)
        .expect("one source retains layer structure");
    let info = project
        .document_font_info(source_id)
        .expect("one source retains canonical font information");
    let metrics = info.metrics.resolved();
    let metadata = project
        .document_font_metadata(source_id)
        .expect("one source retains canonical metadata");
    if json {
        let mut out = json!({
            "ok": true,
            "source": source,
            "family": info.names.family_name,
            "style": info.names.style_name,
            "unitsPerEm": metrics.units_per_em,
            "ascender": metrics.ascender,
            "descender": metrics.descender,
            "xHeight": info.metrics.x_height,
            "capHeight": info.metrics.cap_height,
            "glyphs": glyph_names.len(),
            "drawn": drawn,
            "layers": layers,
            "kerningPairs": metadata.kerning_pairs().count(),
            "proposals": proposals,
        });
        if list_glyphs {
            out["glyphList"] = glyph_names
                .iter()
                .map(|name| {
                    let layer = project
                        .document_layer(name, &default_layer)
                        .expect("collected default-layer glyph");
                    json!({ "name": name, "codepoints": codepoints(layer.codepoints()) })
                })
                .collect();
        }
        println!("{out}");
    } else {
        println!(
            "{} {}",
            info.names.family_name.as_deref().unwrap_or("(no family)"),
            info.names.style_name.as_deref().unwrap_or("")
        );
        println!(
            "{} upm, ascender {}, descender {}",
            metrics.units_per_em, metrics.ascender, metrics.descender
        );
        println!(
            "{} glyphs, {drawn} drawn, layers: {}",
            glyph_names.len(),
            layers.join(", ")
        );
        for p in &proposals {
            println!(
                "proposal {}: {} glyphs ({} compatible, {} not, {} missing)",
                p.task,
                p.glyphs.len(),
                p.compatible.len(),
                p.incompatible.len(),
                p.missing.len()
            );
        }
        if list_glyphs {
            for name in glyph_names {
                let glyph = project
                    .document_layer(name, &default_layer)
                    .expect("collected default-layer glyph");
                println!("  {name:<24} {}", codepoints(glyph.codepoints()).join(" "));
            }
        }
    }
    exit::OK
}

/// A proof sheet: every glyph in a grid with its metric lines, as
/// SVG, and the numbers a reviewer wants next to it.
pub(in crate::application::cli) fn proof(
    source: &Path,
    out: Option<&Path>,
    glyphs: Option<&[String]>,
    columns: usize,
    layer: Option<&str>,
    json: bool,
) -> i32 {
    let project = match open_project(source, json) {
        Ok(project) => project,
        Err(code) => return code,
    };
    let source_id = project
        .source_id(0)
        .expect("one source has a stable identity");
    let default_layer = project
        .document_source(source_id)
        .expect("one source")
        .default_layer();
    let requested_layer = layer.map(|name| LayerId {
        source: source_id,
        name: name.into(),
    });
    let names: Vec<String> = match glyphs {
        Some(list) => list.to_vec(),
        None if requested_layer.is_some() => {
            let requested = requested_layer.as_ref().expect("checked requested layer");
            if !project
                .document_source_layer_names(source_id)
                .is_some_and(|names| names.iter().any(|name| *name == requested.name))
            {
                return fail(json, exit::USAGE, "no such layer");
            }
            project
                .glyph_names()
                .filter(|name| project.document_layer(name, requested).is_some())
                .map(str::to_owned)
                .collect()
        }
        None => project
            .glyph_names()
            .filter(|name| {
                project
                    .document_layer_path(&GlyphLayerAddress {
                        glyph: (*name).to_owned(),
                        layer: default_layer.clone(),
                    })
                    .is_ok_and(|path| !path.is_empty())
            })
            .map(str::to_owned)
            .collect(),
    };
    if names.is_empty() {
        return fail(json, exit::USAGE, "no glyph to draw");
    }
    let sheet = match runebender::formats::svg::proof_sheet_project(
        &project, source_id, layer, &names, columns,
    ) {
        Ok(s) => s,
        Err(e) => return fail(json, exit::USAGE, &e),
    };
    let (svg, metrics) = (sheet.svg, sheet.metrics);
    let out = out.map_or_else(
        || {
            source
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("proof.svg")
        },
        Path::to_path_buf,
    );
    if let Err(e) = std::fs::write(&out, svg) {
        return fail(json, exit::FAILED, &format!("{}: {e}", out.display()));
    }
    if json {
        println!("{}", json!({ "ok": true, "svg": out, "glyphs": metrics }));
    } else {
        println!("{} glyphs → {}", names.len(), out.display());
        for m in &metrics {
            println!(
                "  {:<24} advance {:>5}  lsb {:>5}  rsb {:>5}",
                m["glyph"].as_str().unwrap_or(""),
                m["advance"],
                m["lsb"],
                m["rsb"]
            );
        }
    }
    exit::OK
}

/// Return an actual MCP image alongside proof metadata, without external resources.
#[allow(
    clippy::cast_possible_truncation,
    reason = "Raster dimensions are rounded and bounded to 2048 pixels"
)]
pub(in crate::application::cli) fn proof_content(
    mut value: serde_json::Value,
) -> Vec<serde_json::Value> {
    use base64::Engine as _;
    let mut content = Vec::new();
    if let Some(png) = value
        .as_object_mut()
        .and_then(|object| object.remove("png_base64"))
    {
        content.push(serde_json::json!({"type":"image", "mimeType":"image/png", "data":png}));
    } else if let Some(scene) = value.get("scene") {
        let rendered = runebender::formats::designbot::render(scene, false);
        match rendered {
            Ok(png) => {
                content.push(serde_json::json!({"type":"image", "mimeType":"image/png",
                    "data":base64::engine::general_purpose::STANDARD.encode(png)}));
                value.as_object_mut().unwrap().remove("svg_content");
                value.as_object_mut().unwrap().remove("scene");
            }
            Err(error) => value["image_error"] = serde_json::json!(error),
        }
    }
    content.insert(
        0,
        serde_json::json!({"type":"text", "text":value.to_string()}),
    );
    content
}
