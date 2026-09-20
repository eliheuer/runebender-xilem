// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Data-only Designbot scenes and bounded rendering through its installed CLI.

use serde_json::{Value, json};

use crate::document::LayerView;
use crate::document::canonical_metadata::CanonicalFontMetadata;
use crate::document::experiments::Experiment;
use crate::document::project::Project;
use crate::document::variable::{GlyphLayerAddress, LayerId, SourceId};

/// Build an isolated-glyph proof from the supplied source font or proposal layer.
/// Geometry is resolved before transport; no source files are read by Designbot.
#[allow(
    clippy::cast_possible_truncation,
    reason = "Scene dimensions are bounded positive integers"
)]
#[cfg(test)]
pub fn scene(font: &norad::Font, layer: Option<&str>, names: &[String]) -> Result<Value, String> {
    use kurbo::Affine;
    if names.is_empty() || names.len() > 256 {
        return Err("proof requires 1 to 256 glyphs".into());
    }
    let preview = layer
        .map(|l| crate::document::proposal::preview_font(font, l))
        .transpose()?;
    let font = preview.as_ref().unwrap_or(font);
    let columns = names.len().min(6);
    let rows = names.len().div_ceil(columns);
    let cell = 256.0;
    let height = (rows as f64 * cell).min(2048.0);
    let reduction = height / (rows as f64 * cell);
    let units_per_em = font
        .font_info
        .units_per_em
        .map(|value| value.as_f64())
        .unwrap_or(1000.0);
    let scale = 190.0 / units_per_em * reduction;
    let mut paths = Vec::new();
    let mut labels = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let glyph = font
            .get_glyph(name)
            .ok_or_else(|| format!("no glyph named {name}"))?;
        let x = ((i % columns) as f64 * cell + 24.0) * reduction;
        let y = height - ((i / columns) as f64 * cell + 205.0) * reduction;
        let path = Affine::translate((x, y))
            * Affine::scale(scale)
            * crate::outline::glyph_paths::glyph_to_bezpath(glyph, font);
        paths.push(json!({"d":path.to_svg()}));
        labels
            .push(json!({"text":name,"x":x,"y":y-24.0*reduction,"size":(12.0*reduction).max(1.0)}));
    }
    Ok(
        json!({"version":1,"width":(columns as f64*cell*reduction).round() as u64,"height":height.round() as u64,"paths":paths,"labels":labels}),
    )
}

/// Build an isolated-glyph scene directly from one canonical Project source.
pub fn scene_project(
    project: &Project,
    source: SourceId,
    layer: Option<&str>,
    names: &[String],
) -> Result<Value, String> {
    let default = project
        .document_source(source)
        .ok_or("unknown source")?
        .default_layer();
    let selected = layer.map(|name| LayerId {
        source,
        name: name.to_owned(),
    });
    if let Some(selected) = &selected
        && !project
            .document_source_layer_names(source)
            .is_some_and(|names| names.contains(&selected.name.as_str()))
    {
        return Err(format!("no layer named {}", selected.name));
    }
    let units_per_em = project
        .document_font_info(source)
        .ok_or("unknown source font information")?
        .metrics
        .resolved()
        .units_per_em;
    scene_canonical(units_per_em, names, |name| {
        let layer = selected
            .as_ref()
            .filter(|layer| project.document_layer(name, layer).is_some())
            .unwrap_or(&default);
        project
            .document_layer_path(&GlyphLayerAddress {
                glyph: name.to_owned(),
                layer: layer.clone(),
            })
            .map_err(|error| error.to_string())
    })
}

/// Build an isolated-glyph scene from one canonical experiment snapshot.
pub fn scene_experiment(
    project: &Project,
    experiment: &Experiment,
    layer: Option<&str>,
    names: &[String],
) -> Result<Value, String> {
    if let Some(layer) = layer
        && !experiment
            .layer_drafts()
            .any(|(address, _)| address.layer.name == layer)
    {
        return Err(format!("no layer named {layer}"));
    }
    let units_per_em = project
        .document_font_info(experiment.root)
        .ok_or("unknown experiment source font information")?
        .metrics
        .resolved()
        .units_per_em;
    scene_canonical(units_per_em, names, |name| {
        let (selected, _) = experiment
            .selected_layer(name, layer)
            .or_else(|| experiment.selected_layer(name, None))
            .ok_or_else(|| format!("no glyph named {name}"))?;
        experiment
            .layer_path(name, &selected)
            .map_err(|error| error.to_string())
    })
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Scene dimensions are bounded positive integers"
)]
fn scene_canonical(
    units_per_em: f64,
    names: &[String],
    mut path: impl FnMut(&str) -> Result<kurbo::BezPath, String>,
) -> Result<Value, String> {
    use kurbo::Affine;
    if names.is_empty() || names.len() > 256 {
        return Err("proof requires 1 to 256 glyphs".into());
    }
    let columns = names.len().min(6);
    let rows = names.len().div_ceil(columns);
    let cell = 256.0;
    let height = (rows as f64 * cell).min(2048.0);
    let reduction = height / (rows as f64 * cell);
    let scale = 190.0 / units_per_em * reduction;
    let mut paths = Vec::new();
    let mut labels = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let x = ((index % columns) as f64 * cell + 24.0) * reduction;
        let y = height - ((index / columns) as f64 * cell + 205.0) * reduction;
        let path = Affine::translate((x, y)) * Affine::scale(scale) * path(name)?;
        paths.push(json!({"d":path.to_svg()}));
        labels
            .push(json!({"text":name,"x":x,"y":y-24.0*reduction,"size":(12.0*reduction).max(1.0)}));
    }
    Ok(
        json!({"version":1,"width":(columns as f64*cell*reduction).round() as u64,"height":height.round() as u64,"paths":paths,"labels":labels}),
    )
}

/// Render a version-1 scene as raw PNG or PDF. Runs off the UI thread.
/// Uses `DESIGNBOT_BIN` or `designbot` on PATH. Times out after 30 seconds;
/// temporary inputs and output are removed on success or failure.
pub fn render(scene: &Value, pdf: bool) -> Result<Vec<u8>, String> {
    use std::io::{Read as _, Write as _};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::{
        fs,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "runebender-render-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let input = dir.join("scene.json");
    let output = dir.join(if pdf { "proof.pdf" } else { "proof.png" });
    let errors = dir.join("errors.txt");
    let data = serde_json::to_vec(scene).map_err(|e| e.to_string())?;
    if data.len() > 8 * 1024 * 1024 {
        return Err("scene exceeds 8 MiB".into());
    }
    fs::File::create(&input)
        .and_then(|mut f| f.write_all(&data))
        .map_err(|e| e.to_string())?;
    let mut child =
        Command::new(std::env::var_os("DESIGNBOT_BIN").unwrap_or_else(|| "designbot".into()))
            .args(["render-scene", if pdf { "--pdf" } else { "--png" }])
            .arg(&output)
            .stdin(fs::File::open(input).map_err(|e| e.to_string())?)
            .stdout(Stdio::null())
            .stderr(fs::File::create(&errors).map_err(|e| e.to_string())?)
            .spawn()
            .map_err(|e| {
                format!("Designbot unavailable: {e}; install a version supporting render-scene")
            })?;
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                if !status.success() {
                    let mut message = String::new();
                    let _ =
                        fs::File::open(errors).map(|f| f.take(4096).read_to_string(&mut message));
                    return Err(format!("Designbot failed: {message}"));
                }
                break;
            }
            None if start.elapsed() > Duration::from_secs(30) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Designbot rendering timed out".into());
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    if fs::metadata(&output).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("proof exceeds 32 MiB".into());
    }
    fs::read(output).map_err(|e| e.to_string())
}

/// A one-page Latin kerning specimen from live outlines, shaped with harfrust.
/// Uses canonical kerning with `kern` disabled in feature shaping to avoid double kerning.
/// Other feature positioning remains active. Fails on unsupported text or feature errors.
#[cfg(test)]
pub fn specimen(font: &norad::Font, text: &str) -> Result<Value, String> {
    use crate::text::shape::{ShapingFont, ShapingGlyph, ShapingSource};
    use kurbo::Affine;
    if text.is_empty()
        || text.len() > 256
        || !text.is_ascii()
        || text.chars().any(|c| c.is_control() && c != '\n')
    {
        return Err(
            "MVP text proofs accept 1 to 256 basic Latin characters, with optional newlines".into(),
        );
    }
    let units_per_em = font
        .font_info
        .units_per_em
        .map(|value| value.as_f64())
        .unwrap_or(1000.0);
    let mut glyphs: Vec<_> = font
        .default_layer()
        .iter()
        .map(|g| ShapingGlyph {
            name: g.name().to_string(),
            advance: g.width,
            unicodes: g.codepoints.iter().map(u32::from).collect(),
        })
        .collect();
    glyphs.sort_by_key(|g| (g.name != ".notdef", g.name.clone()));
    if glyphs.first().is_none_or(|g| g.name != ".notdef") {
        glyphs.insert(
            0,
            ShapingGlyph {
                name: ".notdef".into(),
                advance: units_per_em * 0.5,
                unicodes: vec![],
            },
        );
    }
    let shaper = ShapingFont::build(&ShapingSource {
        units_per_em,
        glyphs,
        features: font.features.clone(),
    })?;
    let mut paths = Vec::new();
    let mut labels = Vec::new();
    let mut baseline = 742.0;
    for size in [18.0, 24.0, 36.0, 48.0] {
        let scale = size / units_per_em;
        labels.push(json!({"text":format!("{size} pt / live UFO kerning"),"x":30,"y":baseline+16.0,"size":10}));
        for line in text.lines() {
            let shaped = shaper.shape_with_features(line, false, &[("kern".into(), false)])?;
            let mut x = 30.0;
            let mut previous: Option<&str> = None;
            for item in shaped {
                let name = shaper
                    .glyph_name(item.glyph_id)
                    .ok_or("invalid shaped glyph")?;
                let glyph = font
                    .get_glyph(name)
                    .ok_or_else(|| format!("missing glyph {name}"))?;
                if name == ".notdef" {
                    return Err("specimen contains an unmapped character".into());
                }
                let kern = previous.map_or(0.0, |left| pair_kerning(font, left, name));
                if x + (kern + item.x_advance) * scale > 582.0 {
                    x = 30.0;
                    baseline -= size * 1.5;
                    previous = None;
                }
                if baseline < 40.0 {
                    return Err("specimen exceeds one page; use shorter text".into());
                }
                if previous.is_some() {
                    x += kern * scale;
                }
                let path = Affine::translate((
                    x + item.x_offset * scale,
                    baseline + item.y_offset * scale,
                )) * Affine::scale(scale)
                    * crate::outline::glyph_paths::glyph_to_bezpath(glyph, font);
                paths.push(json!({"d":path.to_svg()}));
                x += item.x_advance * scale;
                previous = Some(name);
            }
            baseline -= size * 1.5;
        }
        baseline -= 32.0;
    }
    Ok(json!({"version":1,"width":612,"height":792,"paths":paths,"labels":labels}))
}

/// Build a one-page specimen from one canonical Project source.
pub fn specimen_project(project: &Project, source: SourceId, text: &str) -> Result<Value, String> {
    let default = project
        .document_source(source)
        .ok_or("unknown source")?
        .default_layer();
    let info = project
        .document_font_info(source)
        .ok_or("unknown source font information")?;
    let layers = project
        .glyph_names()
        .filter_map(|name| {
            project
                .document_layer(name, &default)
                .map(|layer| (name.to_owned(), layer))
        })
        .collect();
    specimen_canonical(
        info.metrics.resolved().units_per_em,
        project.document_feature_text(source).unwrap_or_default(),
        layers,
        project
            .document_font_metadata(source)
            .ok_or("unknown source metadata")?,
        |name| {
            project
                .document_layer_path(&GlyphLayerAddress {
                    glyph: name.to_owned(),
                    layer: default.clone(),
                })
                .map_err(|error| error.to_string())
        },
        text,
    )
}

/// Build a one-page specimen from one canonical experiment snapshot.
pub fn specimen_experiment(
    project: &Project,
    experiment: &Experiment,
    text: &str,
) -> Result<Value, String> {
    let info = project
        .document_font_info(experiment.root)
        .ok_or("unknown experiment source font information")?;
    let names = experiment
        .layer_drafts()
        .map(|(address, _)| address.glyph.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let layers = names
        .into_iter()
        .filter_map(|name| {
            experiment
                .selected_layer(&name, None)
                .map(|(_, layer)| (name, layer))
        })
        .collect();
    specimen_canonical(
        info.metrics.resolved().units_per_em,
        project
            .document_feature_text(experiment.root)
            .unwrap_or_default(),
        layers,
        experiment.font_metadata(),
        |name| {
            let (selected, _) = experiment
                .selected_layer(name, None)
                .ok_or_else(|| format!("missing glyph {name}"))?;
            experiment
                .layer_path(name, &selected)
                .map_err(|error| error.to_string())
        },
        text,
    )
}

fn specimen_canonical<'a>(
    units_per_em: f64,
    features: &str,
    layers: Vec<(String, LayerView<'a>)>,
    metadata: &CanonicalFontMetadata,
    mut path: impl FnMut(&str) -> Result<kurbo::BezPath, String>,
    text: &str,
) -> Result<Value, String> {
    use crate::text::shape::{ShapingFont, ShapingGlyph, ShapingSource};
    use kurbo::Affine;
    if text.is_empty()
        || text.len() > 256
        || !text.is_ascii()
        || text
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err(
            "MVP text proofs accept 1 to 256 basic Latin characters, with optional newlines".into(),
        );
    }
    let available = layers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut glyphs = layers
        .iter()
        .map(|(name, layer)| ShapingGlyph {
            name: name.clone(),
            advance: layer.width(),
            unicodes: layer.codepoints().map(u32::from).collect(),
        })
        .collect::<Vec<_>>();
    glyphs.sort_by_key(|glyph| (glyph.name != ".notdef", glyph.name.clone()));
    if glyphs.first().is_none_or(|glyph| glyph.name != ".notdef") {
        glyphs.insert(
            0,
            ShapingGlyph {
                name: ".notdef".into(),
                advance: units_per_em * 0.5,
                unicodes: vec![],
            },
        );
    }
    let shaper = ShapingFont::build(&ShapingSource {
        units_per_em,
        glyphs,
        features: features.to_owned(),
    })?;
    let mut paths = Vec::new();
    let mut labels = Vec::new();
    let mut baseline = 742.0;
    for size in [18.0, 24.0, 36.0, 48.0] {
        let scale = size / units_per_em;
        labels.push(json!({"text":format!("{size} pt / live UFO kerning"),"x":30,"y":baseline+16.0,"size":10}));
        for line in text.lines() {
            let shaped = shaper.shape_with_features(line, false, &[("kern".into(), false)])?;
            let mut x = 30.0;
            let mut previous: Option<&str> = None;
            for item in shaped {
                let name = shaper
                    .glyph_name(item.glyph_id)
                    .ok_or("invalid shaped glyph")?;
                if name == ".notdef" || !available.contains(name) {
                    return Err("specimen contains an unmapped character".into());
                }
                let kern = previous
                    .and_then(|left| metadata.resolved_kerning(left, name))
                    .unwrap_or(0.0);
                if x + (kern + item.x_advance) * scale > 582.0 {
                    x = 30.0;
                    baseline -= size * 1.5;
                    previous = None;
                }
                if baseline < 40.0 {
                    return Err("specimen exceeds one page; use shorter text".into());
                }
                if previous.is_some() {
                    x += kern * scale;
                }
                let glyph_path = Affine::translate((
                    x + item.x_offset * scale,
                    baseline + item.y_offset * scale,
                )) * Affine::scale(scale)
                    * path(name)?;
                paths.push(json!({"d":glyph_path.to_svg()}));
                x += item.x_advance * scale;
                previous = Some(name);
            }
            baseline -= size * 1.5;
        }
        baseline -= 32.0;
    }
    Ok(json!({"version":1,"width":612,"height":792,"paths":paths,"labels":labels}))
}

/// Resolve pair exceptions before side-specific UFO group pairs.
#[cfg(test)]
fn pair_kerning(font: &norad::Font, left: &str, right: &str) -> f64 {
    let left_group = font
        .groups
        .iter()
        .find(|(name, members)| {
            name.starts_with("public.kern1.") && members.iter().any(|g| g.as_str() == left)
        })
        .map(|(name, _)| name.as_str());
    let right_group = font
        .groups
        .iter()
        .find(|(name, members)| {
            name.starts_with("public.kern2.") && members.iter().any(|g| g.as_str() == right)
        })
        .map(|(name, _)| name.as_str());
    for (l, r) in [
        (Some(left), Some(right)),
        (Some(left), right_group),
        (left_group, Some(right)),
        (left_group, right_group),
    ] {
        if let (Some(l), Some(r)) = (l, r)
            && let Some(value) = font.kerning.get(l).and_then(|row| row.get(r))
        {
            return *value;
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape as _;

    #[test]
    fn live_kerning_changes_positioned_outlines_in_the_scene() {
        let mut font = norad::Font::new();
        for name in [".notdef", "A", "V"] {
            let mut glyph = norad::Glyph::new(name);
            if let Some(codepoint) = name.chars().next().filter(|_| name.len() == 1) {
                glyph.codepoints.insert(codepoint);
            }
            font.default_layer_mut().insert_glyph(glyph);
        }
        for name in ["A", "V"] {
            let glyph = font.get_glyph_mut(name).unwrap();
            glyph.width = 600.0;
            glyph.contours.push(norad::Contour::new(
                [(0.0, 0.0), (250.0, 700.0), (500.0, 0.0)]
                    .into_iter()
                    .map(|(x, y)| {
                        norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None)
                    })
                    .collect(),
                None,
            ));
        }
        let before = specimen(&font, "AV").unwrap();
        font.kerning
            .entry(norad::Name::new("A").unwrap())
            .or_default()
            .insert(norad::Name::new("V").unwrap(), -40.0);
        let after = specimen(&font, "AV").unwrap();
        let x = |v: &Value| {
            kurbo::BezPath::from_svg(v["paths"][1]["d"].as_str().unwrap())
                .unwrap()
                .bounding_box()
                .x0
        };
        assert!((x(&after) - x(&before) + 40.0 * 18.0 / 1000.0).abs() < 1e-6);
        assert_eq!(before["width"], 612);
        assert_eq!(before["height"], 792);
    }
}
