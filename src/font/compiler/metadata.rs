// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO metadata and Designspace rules supplied to the live compiler.

use super::super::model::font_info::{CanonicalFontInfo, OpenTypeWidthClass};
use super::super::model::glyph_metadata::OpenTypeGlyphCategory;

/// Quantize an exact editable units-per-em value for OpenType compilation.
pub(super) fn units_per_em(value: f64) -> Result<u16, String> {
    if !value.is_finite() {
        return Err("units per em must be finite".into());
    }
    let rounded = value.round();
    if !(16.0..=16384.0).contains(&rounded) {
        return Err("units per em must be between 16 and 16384".into());
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the rounded value was checked against the complete accepted u16 subrange"
    )]
    Ok(rounded as u16)
}

/// Quantize an exact editable metric for Babelfont's integer compiler snapshot.
pub(super) fn metric(name: &str, value: f64) -> Result<i32, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    let rounded = value.round();
    if rounded < f64::from(i32::MIN) || rounded > f64::from(i32::MAX) {
        return Err(format!("{name} is outside the OpenType metric range"));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the rounded value was checked against the complete i32 range"
    )]
    Ok(rounded as i32)
}

/// Quantize one exact source kerning value for Babelfont's compiler snapshot.
pub(super) fn kerning(value: f64) -> Result<i16, String> {
    if !value.is_finite() {
        return Err("kerning must be finite".into());
    }
    let rounded = value.round();
    if rounded < f64::from(i16::MIN) || rounded > f64::from(i16::MAX) {
        return Err("kerning is outside the OpenType compiler range".into());
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the rounded value was checked against the complete i16 range"
    )]
    Ok(rounded as i16)
}

/// Copy source groups into Babelfont's two compiler-side kerning-group maps.
///
/// The iterator uses owned strings so canonical metadata can feed this immutable snapshot without
/// coupling the compiler adapter to document storage.
pub(super) fn apply_groups(
    font: &mut babelfont::Font,
    groups: impl IntoIterator<Item = (String, Vec<String>)>,
) {
    font.first_kern_groups.clear();
    font.second_kern_groups.clear();
    for (name, members) in groups {
        let destination = if name.starts_with("public.kern1.") {
            &mut font.first_kern_groups
        } else if name.starts_with("public.kern2.") {
            &mut font.second_kern_groups
        } else {
            continue;
        };
        destination.insert(name.into(), members.into_iter().map(Into::into).collect());
    }
}

/// Quantize exact kerning pairs into one immutable Babelfont master.
pub(super) fn apply_kerning(
    master: &mut babelfont::Master,
    pairs: impl IntoIterator<Item = (String, String, f64)>,
) -> Result<(), String> {
    for (left, right, value) in pairs {
        let participant = |name: String| {
            if name.starts_with("public.kern") {
                format!("@{name}")
            } else {
                name
            }
        };
        master.kerning.insert(
            (participant(left).into(), participant(right).into()),
            kerning(value)?,
        );
    }
    Ok(())
}

/// Resolve a compiler category from canonical explicit data or inferred layer values.
pub(super) fn glyph_category_from_values<'a>(
    glyph_name: &str,
    explicit: Option<&OpenTypeGlyphCategory>,
    codepoints: impl IntoIterator<Item = char>,
    anchor_names: impl IntoIterator<Item = &'a str>,
) -> Result<babelfont::GlyphCategory, String> {
    use babelfont::GlyphCategory;
    match explicit {
        Some(OpenTypeGlyphCategory::Base) => return Ok(GlyphCategory::Base),
        Some(OpenTypeGlyphCategory::Mark) => return Ok(GlyphCategory::Mark),
        Some(OpenTypeGlyphCategory::Ligature) => return Ok(GlyphCategory::Ligature),
        Some(category) => {
            return Err(format!(
                "{glyph_name}: unsupported OpenType category {}",
                category.as_source()
            ));
        }
        None => (),
    }
    let anchor_names: Vec<_> = anchor_names.into_iter().collect();
    let mark = anchor_names.iter().any(|name| name.starts_with('_'))
        || codepoints.into_iter().any(|codepoint| {
            matches!(
                unicode_general_category::get_general_category(codepoint),
                unicode_general_category::GeneralCategory::NonspacingMark
                    | unicode_general_category::GeneralCategory::SpacingMark
                    | unicode_general_category::GeneralCategory::EnclosingMark
            )
        });
    let ligature = anchor_names.iter().any(|name| {
        name.rsplit_once('_')
            .is_some_and(|(_, suffix)| suffix.parse::<usize>().is_ok_and(|index| index > 0))
    });
    Ok(if mark {
        GlyphCategory::Mark
    } else if ligature {
        GlyphCategory::Ligature
    } else {
        GlyphCategory::Base
    })
}

pub(super) fn apply(font: &mut babelfont::Font, info: &CanonicalFontInfo) -> Result<(), String> {
    macro_rules! names {
        ($($target:ident => $source:ident),* $(,)?) => {
            $(font.names.$target = info.names.$source.as_ref().map_or_else(
                Default::default,
                |value| value.as_str().into(),
            );)*
        };
    }
    names! {
        copyright => copyright, trademark => trademark, designer => designer,
        designer_url => designer_url, manufacturer => manufacturer,
        manufacturer_url => manufacturer_url, description => description,
        license => license, license_url => license_url, version => version,
        unique_id => unique_id, sample_text => sample_text, full_name => full_name,
        postscript_name => postscript_name, typographic_family => typographic_family,
        typographic_subfamily => typographic_subfamily, wws_family_name => wws_family_name,
        wws_subfamily_name => wws_subfamily_name,
    }
    font.note.clone_from(&info.note);
    font.version = (
        u16::try_from(info.version_major.unwrap_or(1)).map_err(|_| "invalid major version")?,
        u16::try_from(info.version_minor.unwrap_or(0)).map_err(|_| "invalid minor version")?,
    );
    let bits = |values: &Option<Vec<u8>>| -> Option<u16> {
        values.as_ref().map(|bits| {
            bits.iter()
                .filter(|bit| **bit < 16)
                .fold(0, |value, bit| value | (1 << bit))
        })
    };
    font.custom_ot_values.head_flags = bits(&info.open_type.head_flags);
    font.custom_ot_values.os2_fs_type = bits(&info.open_type.fs_type);
    font.custom_ot_values.os2_fs_selection = bits(&info.open_type.fs_selection);
    font.custom_ot_values.os2_us_weight_class = info
        .open_type
        .weight_class
        .map(u16::try_from)
        .transpose()
        .map_err(|_| "weight class outside OpenType range")?;
    font.custom_ot_values.os2_us_width_class =
        info.open_type.width_class.map(|value| match value {
            OpenTypeWidthClass::UltraCondensed => 1,
            OpenTypeWidthClass::ExtraCondensed => 2,
            OpenTypeWidthClass::Condensed => 3,
            OpenTypeWidthClass::SemiCondensed => 4,
            OpenTypeWidthClass::Normal => 5,
            OpenTypeWidthClass::SemiExpanded => 6,
            OpenTypeWidthClass::Expanded => 7,
            OpenTypeWidthClass::ExtraExpanded => 8,
            OpenTypeWidthClass::UltraExpanded => 9,
        });
    font.custom_ot_values.os2_vendor_id = match &info.open_type.vendor_id {
        Some(vendor) => Some(babelfont::Tag::new(
            vendor
                .as_bytes()
                .try_into()
                .map_err(|_| "vendor ID must have four bytes")?,
        )),
        None => None,
    };
    Ok(())
}

pub(super) fn metrics(
    master: &mut babelfont::Master,
    info: &CanonicalFontInfo,
) -> Result<(), String> {
    master.metrics.clear();
    macro_rules! metrics {
        ($($target:ident => $source:ident),* $(,)?) => {
            $(if let Some(value) = info.open_type_metrics.$source {
                master.metrics.insert(babelfont::MetricType::$target, value);
            })*
        };
    }
    metrics! {
        HheaAscender => hhea_ascender, HheaDescender => hhea_descender,
        HheaLineGap => hhea_line_gap, HheaCaretSlopeRise => hhea_caret_slope_rise,
        HheaCaretSlopeRun => hhea_caret_slope_run, HheaCaretOffset => hhea_caret_offset,
        TypoAscender => typo_ascender, TypoDescender => typo_descender,
        TypoLineGap => typo_line_gap, SubscriptXSize => subscript_x_size,
        SubscriptYSize => subscript_y_size, SubscriptXOffset => subscript_x_offset,
        SubscriptYOffset => subscript_y_offset, SuperscriptXSize => superscript_x_size,
        SuperscriptYSize => superscript_y_size, SuperscriptXOffset => superscript_x_offset,
        SuperscriptYOffset => superscript_y_offset, StrikeoutSize => strikeout_size,
        StrikeoutPosition => strikeout_position,
    }
    for (key, value) in [
        (
            babelfont::MetricType::WinAscent,
            info.open_type_metrics.win_ascent,
        ),
        (
            babelfont::MetricType::WinDescent,
            info.open_type_metrics.win_descent,
        ),
    ] {
        if let Some(value) = value {
            master.metrics.insert(
                key,
                i32::try_from(value).map_err(|_| "metric outside OpenType range")?,
            );
        }
    }
    Ok(())
}

/// Partition rule regions on OpenType's `F2Dot14` lattice.
/// `FeatureVariations` chooses the first matching record, so overlapping records
/// must contain all active rule lookups in document order.
#[expect(
    clippy::cast_possible_truncation,
    reason = "normalized coordinates are bounded to F2Dot14"
)]
pub(super) fn rules(
    structure: Option<&super::super::model::designspace::CanonicalCompilerStructure>,
    font: &mut babelfont::Font,
) -> Result<(), String> {
    use std::fmt::Write as _;
    let Some(structure) = structure else {
        return Ok(());
    };
    if structure.rules.is_empty() {
        return Ok(());
    }
    let domains: Vec<(i32, i32)> = structure
        .axes
        .iter()
        .map(|axis| {
            (
                if axis.design_minimum() < axis.design_default() {
                    -16384
                } else {
                    0
                },
                if axis.design_maximum() > axis.design_default() {
                    16384
                } else {
                    0
                },
            )
        })
        .collect();
    let mut cuts: Vec<std::collections::BTreeSet<i32>> = domains
        .iter()
        .map(|(lo, hi)| [*lo, hi + 1].into())
        .collect();
    let mut regions = Vec::new();
    for rule in &structure.rules {
        let mut sets = Vec::new();
        for set in &rule.condition_sets {
            let mut bounds = domains.clone();
            for condition in &set.conditions {
                let index = structure
                    .axes
                    .iter()
                    .position(|axis| axis.id() == condition.axis)
                    .ok_or("rule refers to an unknown axis")?;
                let axis = &structure.axes[index];
                let min = condition.minimum.unwrap_or_else(|| axis.design_minimum());
                let max = condition.maximum.unwrap_or_else(|| axis.design_maximum());
                if !min.is_finite() || !max.is_finite() || min > max {
                    return Err("invalid Designspace rule bounds".into());
                }
                let lo = (axis.coordinates.design_to_normalized(min) * 16384.0).round() as i32;
                let hi = (axis.coordinates.design_to_normalized(max) * 16384.0).round() as i32;
                bounds[index].0 = bounds[index].0.max(lo);
                bounds[index].1 = bounds[index].1.min(hi);
            }
            if bounds.iter().any(|(lo, hi)| lo > hi) {
                continue;
            }
            for (index, (lo, hi)) in bounds.iter().enumerate() {
                cuts[index].extend([*lo, hi + 1]);
            }
            sets.push(bounds);
        }
        regions.push(sets);
    }
    let mut cells = vec![Vec::<(i32, i32)>::new()];
    for axis_cuts in cuts {
        let values: Vec<_> = axis_cuts.into_iter().collect();
        let segments = values.len().saturating_sub(1);
        if cells.len().saturating_mul(segments) > 65536 {
            return Err("Designspace rules create more than 65536 distinct compile regions".into());
        }
        cells = cells
            .into_iter()
            .flat_map(|cell| {
                values.windows(2).map(move |pair| {
                    let mut next = cell.clone();
                    next.push((pair[0], pair[1] - 1));
                    next
                })
            })
            .collect();
    }
    let mut fea = String::new();
    for (index, rule) in structure.rules.iter().enumerate() {
        if rule.substitutions.is_empty() {
            continue;
        }
        writeln!(fea, "lookup RunebenderRule{index} {{").expect("write to string");
        for sub in &rule.substitutions {
            writeln!(fea, "sub {} by {};", sub.name, sub.replacement).expect("write to string");
        }
        writeln!(fea, "}} RunebenderRule{index};").expect("write to string");
    }
    let feature = match structure.rule_processing {
        super::super::model::designspace::RuleProcessing::First => "rvrn",
        super::super::model::designspace::RuleProcessing::Last => "rclt",
    };
    for (cell_index, cell) in cells.iter().enumerate() {
        let active: Vec<_> = regions
            .iter()
            .enumerate()
            .filter_map(|(index, sets)| {
                (!structure.rules[index].substitutions.is_empty()
                    && sets.iter().any(|bounds| {
                        bounds
                            .iter()
                            .zip(cell)
                            .all(|((lo, hi), (point, _))| lo <= point && point <= hi)
                    }))
                .then_some(index)
            })
            .collect();
        if active.is_empty() {
            continue;
        }
        let name = format!("RunebenderRegion{cell_index}");
        writeln!(fea, "conditionset {name} {{").expect("write to string");
        for (axis, (lo, hi)) in structure.axes.iter().zip(cell) {
            let min = axis
                .coordinates
                .normalized_to_user(f64::from(*lo) / 16384.0);
            let max = axis
                .coordinates
                .normalized_to_user(f64::from(*hi) / 16384.0);
            writeln!(fea, "{} {min} {max};", axis.coordinates.tag).expect("write to string");
        }
        writeln!(fea, "}} {name};\nvariation {feature} {name} {{").expect("write to string");
        for index in active {
            writeln!(fea, "lookup RunebenderRule{index};").expect("write to string");
        }
        writeln!(fea, "}} {feature};").expect("write to string");
    }
    if !fea.is_empty() {
        font.features =
            babelfont::Features::from_fea(&format!("{}\n{fea}", font.features.to_fea()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiler_quantization_is_checked_and_does_not_change_exact_inputs() {
        let upm = 1000.6;
        let ascender = 812.75;
        let kern = -50.5;
        assert_eq!(units_per_em(upm).unwrap(), 1001);
        assert_eq!(metric("ascender", ascender).unwrap(), 813);
        assert_eq!(kerning(kern).unwrap(), -51);
        assert_eq!(upm, 1000.6);
        assert_eq!(ascender, 812.75);
        assert_eq!(kern, -50.5);

        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(units_per_em(invalid).is_err());
            assert!(metric("metric", invalid).is_err());
            assert!(kerning(invalid).is_err());
        }
        assert!(units_per_em(15.49).is_err());
        assert!(units_per_em(16384.5).is_err());
        assert!(metric("metric", f64::from(i32::MAX) + 1.0).is_err());
        assert!(kerning(f64::from(i16::MIN) - 1.0).is_err());
        assert!(kerning(f64::from(i16::MAX) + 1.0).is_err());
    }

    #[test]
    fn compiler_category_accepts_canonical_values_and_rejects_unknown_explicit_data() {
        use super::super::super::model::glyph_metadata::OpenTypeGlyphCategory;

        assert_eq!(
            glyph_category_from_values("acutecomb", None, ['\u{301}'], std::iter::empty()).unwrap(),
            babelfont::GlyphCategory::Mark
        );
        assert_eq!(
            glyph_category_from_values("f_f", None, [], ["top_1", "top_2"]).unwrap(),
            babelfont::GlyphCategory::Ligature
        );
        assert!(
            glyph_category_from_values(
                "future",
                Some(&OpenTypeGlyphCategory::Other("future-category".into())),
                [],
                std::iter::empty()
            )
            .unwrap_err()
            .contains("unsupported OpenType category")
        );
        for unsupported in [
            OpenTypeGlyphCategory::Unassigned,
            OpenTypeGlyphCategory::Component,
        ] {
            assert!(
                glyph_category_from_values(
                    "unsupported",
                    Some(&unsupported),
                    [],
                    std::iter::empty()
                )
                .is_err()
            );
        }
    }
}
