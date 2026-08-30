// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Runebender's own `com.runebender.*` lib keys, and `public.postscriptNames`.
//!
//! Masks, saved sidebar filters, annotations, and HOI intermediates.
//! Each key has one reader and one writer here, so the on-disk format
//! is defined in one place.

use std::collections::HashSet;

use kurbo::BezPath;

use crate::glyph_ops::bezpath_to_contour;
use crate::glyph_paths::contour_to_bezpath;

/// Contour indices marked as masks: shapes that cut away from the
/// rest of the glyph. Live in a lib key; previews subtract them,
/// Bake Masks makes the subtraction real (external compilers only
/// see baked outlines).
pub const MASKS_KEY: &str = "com.runebender.masks";

pub fn read_masks(glyph: &norad::Glyph) -> HashSet<usize> {
    glyph
        .lib
        .get(MASKS_KEY)
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|v| v.as_signed_integer())
                .filter(|&i| i >= 0)
                .map(|i| i as usize)
                .collect()
        })
        .unwrap_or_default()
}

pub fn write_masks(glyph: &mut norad::Glyph, masks: &HashSet<usize>) {
    if masks.is_empty() {
        glyph.lib.remove(MASKS_KEY);
        return;
    }
    let mut sorted: Vec<usize> = masks.iter().copied().collect();
    sorted.sort_unstable();
    glyph.lib.insert(
        MASKS_KEY.into(),
        plist::Value::Array(
            sorted
                .into_iter()
                .map(|i| plist::Value::Integer((i as u64).into()))
                .collect(),
        ),
    );
}

/// Cut the mask contours out of the rest and drop them: the final
/// outline every compiler understands. Returns false when the glyph
/// has no masks or the boolean fails.
pub fn bake_masks(glyph: &mut norad::Glyph) -> bool {
    let masks = read_masks(glyph);
    if masks.is_empty() || masks.len() >= glyph.contours.len() {
        return false;
    }
    let mut keep = BezPath::new();
    let mut cut = BezPath::new();
    for (ci, contour) in glyph.contours.iter().enumerate() {
        let path = contour_to_bezpath(contour);
        let target = if masks.contains(&ci) {
            &mut cut
        } else {
            &mut keep
        };
        target.extend(path.elements().iter().copied());
    }
    let Ok(result) = linesweeper::binary_op(
        &keep,
        &cut,
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Difference,
    ) else {
        return false;
    };
    let empty = std::collections::HashMap::new();
    let mut contours = Vec::new();
    for contour in result.contours() {
        if let Some(c) = bezpath_to_contour(&contour.path, &empty) {
            contours.push(c);
        }
    }
    if contours.is_empty() {
        return false;
    }
    glyph.contours = contours;
    write_masks(glyph, &HashSet::new());
    true
}

/// Editor annotations, the Glyphs annotation tool's marks: arrows,
/// circles, plus/minus, and text notes pinned to design-space
/// points. Stored in a glyph lib key; never exported.
/// Saved sidebar filters: searches the user pinned, stored in the
/// font lib as an array of {name, query} dicts. Glyphs calls these
/// smart filters; ours reuse the search-field predicate language.
pub const SAVED_FILTERS_KEY: &str = "com.runebender.savedFilters";

/// UFO-standard glyph name -> production name mapping (consumed by
/// ufo2ft/fontc at compile time).
pub const PSNAMES_KEY: &str = "public.postscriptNames";

pub fn read_production_name(font: &norad::Font, glyph: &str) -> Option<String> {
    match font.lib.get(PSNAMES_KEY)? {
        plist::Value::Dictionary(d) => d.get(glyph)?.as_string().map(str::to_string),
        _ => None,
    }
}

/// Set or clear one glyph's production name in `public.postscriptNames`.
/// An empty `text` clears it, and removes the dictionary when it was
/// the last entry. Returns true when the lib changed.
pub fn write_production_name(font: &mut norad::Font, glyph: &str, text: &str) -> bool {
    let dict = match font.lib.get_mut(PSNAMES_KEY) {
        Some(plist::Value::Dictionary(d)) => d,
        _ => {
            if text.is_empty() {
                return false;
            }
            font.lib.insert(
                PSNAMES_KEY.into(),
                plist::Value::Dictionary(plist::Dictionary::new()),
            );
            match font.lib.get_mut(PSNAMES_KEY) {
                Some(plist::Value::Dictionary(d)) => d,
                _ => return false,
            }
        }
    };
    let before = dict.get(glyph).and_then(|v| v.as_string());
    if text.is_empty() {
        if before.is_none() {
            return false;
        }
        dict.remove(glyph);
        if dict.is_empty() {
            font.lib.remove(PSNAMES_KEY);
        }
        true
    } else if before != Some(text) {
        dict.insert(glyph.to_string(), plist::Value::String(text.to_string()));
        true
    } else {
        false
    }
}

pub fn read_saved_filters(font: &norad::Font) -> Vec<(String, String)> {
    let Some(plist::Value::Array(rows)) = font.lib.get(SAVED_FILTERS_KEY) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let dict = row.as_dictionary()?;
            let name = dict.get("name")?.as_string()?.to_string();
            let query = dict.get("query")?.as_string()?.to_string();
            Some((name, query))
        })
        .collect()
}

pub fn write_saved_filters(font: &mut norad::Font, filters: &[(String, String)]) {
    if filters.is_empty() {
        font.lib.remove(SAVED_FILTERS_KEY);
        return;
    }
    let rows = filters
        .iter()
        .map(|(name, query)| {
            let mut dict = plist::Dictionary::new();
            dict.insert("name".into(), plist::Value::String(name.clone()));
            dict.insert("query".into(), plist::Value::String(query.clone()));
            plist::Value::Dictionary(dict)
        })
        .collect();
    font.lib
        .insert(SAVED_FILTERS_KEY.into(), plist::Value::Array(rows));
}

pub const ANNOTATIONS_KEY: &str = "com.runebender.annotations";

#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub text: String,
}

pub fn read_annotations(glyph: &norad::Glyph) -> Vec<Annotation> {
    glyph
        .lib
        .get(ANNOTATIONS_KEY)
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let dict = row.as_dictionary()?;
                    Some(Annotation {
                        kind: dict.get("kind")?.as_string()?.to_string(),
                        x: dict.get("x")?.as_real()?,
                        y: dict.get("y")?.as_real()?,
                        text: dict
                            .get("text")
                            .and_then(|t| t.as_string())
                            .unwrap_or_default()
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn write_annotations(glyph: &mut norad::Glyph, notes: &[Annotation]) {
    if notes.is_empty() {
        glyph.lib.remove(ANNOTATIONS_KEY);
        return;
    }
    let rows = notes
        .iter()
        .map(|a| {
            let mut dict = plist::Dictionary::new();
            dict.insert("kind".into(), plist::Value::String(a.kind.clone()));
            dict.insert("x".into(), plist::Value::Real(a.x));
            dict.insert("y".into(), plist::Value::Real(a.y));
            if !a.text.is_empty() {
                dict.insert("text".into(), plist::Value::String(a.text.clone()));
            }
            plist::Value::Dictionary(dict)
        })
        .collect();
    glyph
        .lib
        .insert(ANNOTATIONS_KEY.into(), plist::Value::Array(rows));
}

/// Per-node HOI intermediate points (the Glyphs "Intermediate
/// Point": the node's interpolation path curves through it at the
/// axis middle). Stored on the axis-min master's glyph, absolute
/// design coordinates, keyed "contour,point". Source of truth for
/// re-editing; the baked brace layers are what compilers consume.
pub const HOI_INTERMEDIATE_KEY: &str = "com.runebender.hoiIntermediate";

pub fn read_hoi_intermediates(
    glyph: &norad::Glyph,
) -> std::collections::HashMap<(usize, usize), (f64, f64)> {
    glyph
        .lib
        .get(HOI_INTERMEDIATE_KEY)
        .and_then(|v| v.as_dictionary())
        .map(|dict| {
            dict.iter()
                .filter_map(|(key, value)| {
                    let (c, p) = key.split_once(',')?;
                    let arr = value.as_array()?;
                    let x = arr.first()?.as_real()?;
                    let y = arr.get(1)?.as_real()?;
                    Some(((c.parse().ok()?, p.parse().ok()?), (x, y)))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn write_hoi_intermediates(
    glyph: &mut norad::Glyph,
    map: &std::collections::HashMap<(usize, usize), (f64, f64)>,
) {
    if map.is_empty() {
        glyph.lib.remove(HOI_INTERMEDIATE_KEY);
        return;
    }
    let mut dict = plist::Dictionary::new();
    for ((c, p), (x, y)) in map {
        dict.insert(
            format!("{c},{p}"),
            plist::Value::Array(vec![plist::Value::Real(*x), plist::Value::Real(*y)]),
        );
    }
    glyph
        .lib
        .insert(HOI_INTERMEDIATE_KEY.into(), plist::Value::Dictionary(dict));
}

/// Quadratic through Q at the middle: position at `t` between `a`
/// and `b` when the path must pass through `q` at t = 0.5.
pub fn hoi_quad_at(a: (f64, f64), b: (f64, f64), q: (f64, f64), t: f64) -> (f64, f64) {
    // Control C with (1-t)²A + 2(1-t)tC + t²B passing Q at 0.5:
    // Q = A/4 + C/2 + B/4  =>  C = 2Q - (A+B)/2.
    let c = (2.0 * q.0 - (a.0 + b.0) / 2.0, 2.0 * q.1 - (a.1 + b.1) / 2.0);
    let u = 1.0 - t;
    (
        u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
        u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_names_read_from_lib() {
        let mut font = norad::Font::new();
        assert_eq!(read_production_name(&font, "uni0627"), None);
        let mut dict = plist::Dictionary::new();
        dict.insert("alef-ar".into(), plist::Value::String("uni0627".into()));
        font.lib
            .insert(PSNAMES_KEY.into(), plist::Value::Dictionary(dict));
        assert_eq!(
            read_production_name(&font, "alef-ar").as_deref(),
            Some("uni0627")
        );
        assert_eq!(read_production_name(&font, "beh-ar"), None);
    }
    #[test]
    fn saved_filters_roundtrip() {
        let mut font = norad::Font::new();
        assert!(read_saved_filters(&font).is_empty());
        let filters = vec![
            ("wide".to_string(), "w>600".to_string()),
            ("marks".to_string(), "cat:mark".to_string()),
        ];
        write_saved_filters(&mut font, &filters);
        assert_eq!(read_saved_filters(&font), filters);
        write_saved_filters(&mut font, &[]);
        assert!(font.lib.get(SAVED_FILTERS_KEY).is_none());
    }
    #[test]
    fn masks_roundtrip_and_bake() {
        use norad::{Contour, ContourPoint, PointType};
        let square = |x0: f64, y0: f64, x1: f64, y1: f64| {
            Contour::new(
                [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                    .iter()
                    .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                    .collect(),
                None,
            )
        };
        let mut glyph = norad::Glyph::new("mask-test");
        // A big square with a smaller mask square overlapping its
        // right edge.
        glyph.contours = vec![
            square(0.0, 0.0, 100.0, 100.0),
            square(60.0, 20.0, 140.0, 80.0),
        ];
        let mut masks = std::collections::HashSet::new();
        masks.insert(1usize);
        write_masks(&mut glyph, &masks);
        assert_eq!(read_masks(&glyph), masks);
        assert!(bake_masks(&mut glyph));
        // The bite is real: no point reaches past x=60 inside the
        // mask's y-band, and the mask key is cleared.
        assert!(read_masks(&glyph).is_empty());
        let max_x_in_band = glyph
            .contours
            .iter()
            .flat_map(|c| c.points.iter())
            .filter(|p| p.y > 25.0 && p.y < 75.0)
            .map(|p| p.x)
            .fold(f64::MIN, f64::max);
        assert!(
            max_x_in_band <= 61.0,
            "mask cut the right side: {max_x_in_band}"
        );
    }
    #[test]
    fn annotations_roundtrip() {
        let mut glyph = norad::Glyph::new("anno");
        let notes = vec![
            Annotation {
                kind: "arrow".into(),
                x: 10.0,
                y: 20.0,
                text: String::new(),
            },
            Annotation {
                kind: "note".into(),
                x: -5.0,
                y: 700.0,
                text: "fix this join".into(),
            },
        ];
        write_annotations(&mut glyph, &notes);
        assert_eq!(read_annotations(&glyph), notes);
        write_annotations(&mut glyph, &[]);
        assert!(glyph.lib.get(ANNOTATIONS_KEY).is_none());
    }
    #[test]
    fn hoi_quad_passes_through_the_intermediate() {
        let a = (0.0, 0.0);
        let b = (100.0, 0.0);
        let q = (50.0, 40.0);
        assert_eq!(hoi_quad_at(a, b, q, 0.0), a);
        assert_eq!(hoi_quad_at(a, b, q, 1.0), b);
        assert_eq!(hoi_quad_at(a, b, q, 0.5), q);
        // Quarter stop, worked by hand: control C = (50, 80).
        let (x, y) = hoi_quad_at(a, b, q, 0.25);
        assert!((x - 25.0).abs() < 1e-9 && (y - 30.0).abs() < 1e-9);
    }
    #[test]
    fn hoi_intermediates_roundtrip_the_lib_key() {
        let mut glyph = norad::Glyph::new("hoi-store");
        let mut map = std::collections::HashMap::new();
        map.insert((0usize, 3usize), (166.0, 73.0));
        map.insert((2, 0), (-12.0, 400.0));
        write_hoi_intermediates(&mut glyph, &map);
        assert_eq!(read_hoi_intermediates(&glyph), map);
        // Empty map clears the key.
        write_hoi_intermediates(&mut glyph, &std::collections::HashMap::new());
        assert!(glyph.lib.get(HOI_INTERMEDIATE_KEY).is_none());
    }
}
