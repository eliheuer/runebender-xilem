// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Exact font-info values used by the editor and compiler.
//!
//! This is the canonical subset of UFO `fontinfo.plist` that Runebender currently reads live.
//! Fields outside this subset remain in the UFO preservation template until they gain a typed
//! owner.

use std::fmt;

/// Human-readable and OpenType name strings used by the current compiler.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalFontNames {
    /// UFO family name.
    pub family_name: Option<String>,
    /// UFO style name.
    pub style_name: Option<String>,
    /// Copyright statement.
    pub copyright: Option<String>,
    /// Trademark statement.
    pub trademark: Option<String>,
    /// Designer name.
    pub designer: Option<String>,
    /// Designer URL.
    pub designer_url: Option<String>,
    /// Manufacturer name.
    pub manufacturer: Option<String>,
    /// Manufacturer URL.
    pub manufacturer_url: Option<String>,
    /// OpenType description.
    pub description: Option<String>,
    /// License text.
    pub license: Option<String>,
    /// License URL.
    pub license_url: Option<String>,
    /// OpenType version string.
    pub version: Option<String>,
    /// OpenType unique identifier.
    pub unique_id: Option<String>,
    /// OpenType sample text.
    pub sample_text: Option<String>,
    /// PostScript full name.
    pub full_name: Option<String>,
    /// PostScript font name.
    pub postscript_name: Option<String>,
    /// Preferred typographic family name.
    pub typographic_family: Option<String>,
    /// Preferred typographic subfamily name.
    pub typographic_subfamily: Option<String>,
    /// WWS family name.
    pub wws_family_name: Option<String>,
    /// WWS subfamily name.
    pub wws_subfamily_name: Option<String>,
}

/// Exact source metrics used directly by the editor and compiler.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanonicalFontMetrics {
    /// Units per em, absent when the UFO relies on the editor default.
    pub units_per_em: Option<f64>,
    /// General ascender.
    pub ascender: Option<f64>,
    /// General descender.
    pub descender: Option<f64>,
    /// x-height.
    pub x_height: Option<f64>,
    /// Cap height.
    pub cap_height: Option<f64>,
    /// Italic angle in counter-clockwise degrees.
    pub italic_angle: Option<f64>,
}

impl CanonicalFontMetrics {
    /// Resolve the current editor defaults without changing the stored optional values.
    pub fn resolved(&self) -> ResolvedFontMetrics {
        let units_per_em = self.units_per_em.unwrap_or(1000.0);
        ResolvedFontMetrics {
            units_per_em,
            ascender: self.ascender.unwrap_or(units_per_em * 0.8),
            descender: self.descender.unwrap_or(-units_per_em * 0.2),
            x_height: self.x_height.unwrap_or(units_per_em * 0.5),
            cap_height: self.cap_height.unwrap_or(units_per_em * 0.7),
        }
    }
}

/// Fully resolved metrics used for editor layout and previews.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedFontMetrics {
    /// Effective units per em.
    pub units_per_em: f64,
    /// Effective ascender.
    pub ascender: f64,
    /// Effective descender.
    pub descender: f64,
    /// Effective x-height.
    pub x_height: f64,
    /// Effective cap height.
    pub cap_height: f64,
}

/// OpenType OS/2 width class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OpenTypeWidthClass {
    /// Ultra-condensed.
    UltraCondensed = 1,
    /// Extra-condensed.
    ExtraCondensed = 2,
    /// Condensed.
    Condensed = 3,
    /// Semi-condensed.
    SemiCondensed = 4,
    /// Medium or normal.
    Normal = 5,
    /// Semi-expanded.
    SemiExpanded = 6,
    /// Expanded.
    Expanded = 7,
    /// Extra-expanded.
    ExtraExpanded = 8,
    /// Ultra-expanded.
    UltraExpanded = 9,
}

/// Exact integer metrics supplied to OpenType compiler tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalOpenTypeMetrics {
    /// hhea ascender.
    pub hhea_ascender: Option<i32>,
    /// hhea descender.
    pub hhea_descender: Option<i32>,
    /// hhea line gap.
    pub hhea_line_gap: Option<i32>,
    /// hhea caret slope rise.
    pub hhea_caret_slope_rise: Option<i32>,
    /// hhea caret slope run.
    pub hhea_caret_slope_run: Option<i32>,
    /// hhea caret offset.
    pub hhea_caret_offset: Option<i32>,
    /// OS/2 typo ascender.
    pub typo_ascender: Option<i32>,
    /// OS/2 typo descender.
    pub typo_descender: Option<i32>,
    /// OS/2 typo line gap.
    pub typo_line_gap: Option<i32>,
    /// OS/2 subscript horizontal size.
    pub subscript_x_size: Option<i32>,
    /// OS/2 subscript vertical size.
    pub subscript_y_size: Option<i32>,
    /// OS/2 subscript horizontal offset.
    pub subscript_x_offset: Option<i32>,
    /// OS/2 subscript vertical offset.
    pub subscript_y_offset: Option<i32>,
    /// OS/2 superscript horizontal size.
    pub superscript_x_size: Option<i32>,
    /// OS/2 superscript vertical size.
    pub superscript_y_size: Option<i32>,
    /// OS/2 superscript horizontal offset.
    pub superscript_x_offset: Option<i32>,
    /// OS/2 superscript vertical offset.
    pub superscript_y_offset: Option<i32>,
    /// OS/2 strikeout size.
    pub strikeout_size: Option<i32>,
    /// OS/2 strikeout position.
    pub strikeout_position: Option<i32>,
    /// OS/2 Windows ascent.
    pub win_ascent: Option<u32>,
    /// OS/2 Windows descent.
    pub win_descent: Option<u32>,
}

/// Exact non-metric OpenType values read by the current compiler.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalOpenTypeInfo {
    /// head table flag bit numbers in source order.
    pub head_flags: Option<Vec<u8>>,
    /// OS/2 embedding flag bit numbers in source order.
    pub fs_type: Option<Vec<u8>>,
    /// OS/2 selection flag bit numbers in source order.
    pub fs_selection: Option<Vec<u8>>,
    /// OS/2 weight class.
    pub weight_class: Option<u32>,
    /// OS/2 width class.
    pub width_class: Option<OpenTypeWidthClass>,
    /// Four-byte OS/2 vendor identifier.
    pub vendor_id: Option<String>,
}

/// Canonical per-source font information used by current Runebender behavior.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CanonicalFontInfo {
    /// Names supplied to the editor and compiler.
    pub names: CanonicalFontNames,
    /// Exact general metrics.
    pub metrics: CanonicalFontMetrics,
    /// Exact OpenType table metrics.
    pub open_type_metrics: CanonicalOpenTypeMetrics,
    /// Exact non-metric OpenType values.
    pub open_type: CanonicalOpenTypeInfo,
    /// Arbitrary source note, preserving absent versus empty.
    pub note: Option<String>,
    /// Major version number.
    pub version_major: Option<i32>,
    /// Minor version number.
    pub version_minor: Option<u32>,
}

/// A rejected canonical font-info boundary value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalFontInfoError {
    /// A floating-point field was NaN or infinite.
    NonFinite(&'static str),
    /// Units per em was negative.
    NegativeUnitsPerEm,
}

impl fmt::Display for CanonicalFontInfoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(field) => write!(formatter, "{field} must be finite"),
            Self::NegativeUnitsPerEm => formatter.write_str("units per em must not be negative"),
        }
    }
}

impl std::error::Error for CanonicalFontInfoError {}

impl CanonicalFontInfo {
    /// Validate editable numeric values without constructing a UFO projection.
    pub fn validate(&self) -> Result<(), CanonicalFontInfoError> {
        validate_metrics(&self.metrics)
    }

    /// Decode the exact live subset from a UFO boundary value.
    pub fn from_ufo(info: &norad::FontInfo) -> Result<Self, CanonicalFontInfoError> {
        let metrics = CanonicalFontMetrics {
            units_per_em: info.units_per_em.map(|value| value.as_f64()),
            ascender: info.ascender,
            descender: info.descender,
            x_height: info.x_height,
            cap_height: info.cap_height,
            italic_angle: info.italic_angle,
        };
        validate_metrics(&metrics)?;
        Ok(Self {
            names: CanonicalFontNames {
                family_name: info.family_name.clone(),
                style_name: info.style_name.clone(),
                copyright: info.copyright.clone(),
                trademark: info.trademark.clone(),
                designer: info.open_type_name_designer.clone(),
                designer_url: info.open_type_name_designer_url.clone(),
                manufacturer: info.open_type_name_manufacturer.clone(),
                manufacturer_url: info.open_type_name_manufacturer_url.clone(),
                description: info.open_type_name_description.clone(),
                license: info.open_type_name_license.clone(),
                license_url: info.open_type_name_license_url.clone(),
                version: info.open_type_name_version.clone(),
                unique_id: info.open_type_name_unique_id.clone(),
                sample_text: info.open_type_name_sample_text.clone(),
                full_name: info.postscript_full_name.clone(),
                postscript_name: info.postscript_font_name.clone(),
                typographic_family: info.open_type_name_preferred_family_name.clone(),
                typographic_subfamily: info.open_type_name_preferred_subfamily_name.clone(),
                wws_family_name: info.open_type_name_wws_family_name.clone(),
                wws_subfamily_name: info.open_type_name_wws_subfamily_name.clone(),
            },
            metrics,
            open_type_metrics: CanonicalOpenTypeMetrics {
                hhea_ascender: info.open_type_hhea_ascender,
                hhea_descender: info.open_type_hhea_descender,
                hhea_line_gap: info.open_type_hhea_line_gap,
                hhea_caret_slope_rise: info.open_type_hhea_caret_slope_rise,
                hhea_caret_slope_run: info.open_type_hhea_caret_slope_run,
                hhea_caret_offset: info.open_type_hhea_caret_offset,
                typo_ascender: info.open_type_os2_typo_ascender,
                typo_descender: info.open_type_os2_typo_descender,
                typo_line_gap: info.open_type_os2_typo_line_gap,
                subscript_x_size: info.open_type_os2_subscript_x_size,
                subscript_y_size: info.open_type_os2_subscript_y_size,
                subscript_x_offset: info.open_type_os2_subscript_x_offset,
                subscript_y_offset: info.open_type_os2_subscript_y_offset,
                superscript_x_size: info.open_type_os2_superscript_x_size,
                superscript_y_size: info.open_type_os2_superscript_y_size,
                superscript_x_offset: info.open_type_os2_superscript_x_offset,
                superscript_y_offset: info.open_type_os2_superscript_y_offset,
                strikeout_size: info.open_type_os2_strikeout_size,
                strikeout_position: info.open_type_os2_strikeout_position,
                win_ascent: info.open_type_os2_win_ascent,
                win_descent: info.open_type_os2_win_descent,
            },
            open_type: CanonicalOpenTypeInfo {
                head_flags: info.open_type_head_flags.clone(),
                fs_type: info.open_type_os2_type.clone(),
                fs_selection: info.open_type_os2_selection.clone(),
                weight_class: info.open_type_os2_weight_class,
                width_class: info.open_type_os2_width_class.map(width_class_from_ufo),
                vendor_id: info.open_type_os2_vendor_id.clone(),
            },
            note: info.note.clone(),
            version_major: info.version_major,
            version_minor: info.version_minor,
        })
    }

    /// Encode this subset into a UFO boundary value atomically.
    ///
    /// Fields outside the canonical subset remain byte-for-byte equivalent values in `info`.
    pub fn write_to_ufo(&self, info: &mut norad::FontInfo) -> Result<bool, CanonicalFontInfoError> {
        self.validate()?;
        let mut output = info.clone();
        output.family_name.clone_from(&self.names.family_name);
        output.style_name.clone_from(&self.names.style_name);
        output.copyright.clone_from(&self.names.copyright);
        output.trademark.clone_from(&self.names.trademark);
        output
            .open_type_name_designer
            .clone_from(&self.names.designer);
        output
            .open_type_name_designer_url
            .clone_from(&self.names.designer_url);
        output
            .open_type_name_manufacturer
            .clone_from(&self.names.manufacturer);
        output
            .open_type_name_manufacturer_url
            .clone_from(&self.names.manufacturer_url);
        output
            .open_type_name_description
            .clone_from(&self.names.description);
        output
            .open_type_name_license
            .clone_from(&self.names.license);
        output
            .open_type_name_license_url
            .clone_from(&self.names.license_url);
        output
            .open_type_name_version
            .clone_from(&self.names.version);
        output
            .open_type_name_unique_id
            .clone_from(&self.names.unique_id);
        output
            .open_type_name_sample_text
            .clone_from(&self.names.sample_text);
        output
            .postscript_full_name
            .clone_from(&self.names.full_name);
        output
            .postscript_font_name
            .clone_from(&self.names.postscript_name);
        output
            .open_type_name_preferred_family_name
            .clone_from(&self.names.typographic_family);
        output
            .open_type_name_preferred_subfamily_name
            .clone_from(&self.names.typographic_subfamily);
        output
            .open_type_name_wws_family_name
            .clone_from(&self.names.wws_family_name);
        output
            .open_type_name_wws_subfamily_name
            .clone_from(&self.names.wws_subfamily_name);

        output.units_per_em = self
            .metrics
            .units_per_em
            .map(norad::fontinfo::NonNegativeIntegerOrFloat::try_from)
            .transpose()
            .map_err(|_| CanonicalFontInfoError::NegativeUnitsPerEm)?;
        output.ascender = self.metrics.ascender;
        output.descender = self.metrics.descender;
        output.x_height = self.metrics.x_height;
        output.cap_height = self.metrics.cap_height;
        output.italic_angle = self.metrics.italic_angle;

        let metrics = &self.open_type_metrics;
        output.open_type_hhea_ascender = metrics.hhea_ascender;
        output.open_type_hhea_descender = metrics.hhea_descender;
        output.open_type_hhea_line_gap = metrics.hhea_line_gap;
        output.open_type_hhea_caret_slope_rise = metrics.hhea_caret_slope_rise;
        output.open_type_hhea_caret_slope_run = metrics.hhea_caret_slope_run;
        output.open_type_hhea_caret_offset = metrics.hhea_caret_offset;
        output.open_type_os2_typo_ascender = metrics.typo_ascender;
        output.open_type_os2_typo_descender = metrics.typo_descender;
        output.open_type_os2_typo_line_gap = metrics.typo_line_gap;
        output.open_type_os2_subscript_x_size = metrics.subscript_x_size;
        output.open_type_os2_subscript_y_size = metrics.subscript_y_size;
        output.open_type_os2_subscript_x_offset = metrics.subscript_x_offset;
        output.open_type_os2_subscript_y_offset = metrics.subscript_y_offset;
        output.open_type_os2_superscript_x_size = metrics.superscript_x_size;
        output.open_type_os2_superscript_y_size = metrics.superscript_y_size;
        output.open_type_os2_superscript_x_offset = metrics.superscript_x_offset;
        output.open_type_os2_superscript_y_offset = metrics.superscript_y_offset;
        output.open_type_os2_strikeout_size = metrics.strikeout_size;
        output.open_type_os2_strikeout_position = metrics.strikeout_position;
        output.open_type_os2_win_ascent = metrics.win_ascent;
        output.open_type_os2_win_descent = metrics.win_descent;

        output
            .open_type_head_flags
            .clone_from(&self.open_type.head_flags);
        output
            .open_type_os2_type
            .clone_from(&self.open_type.fs_type);
        output
            .open_type_os2_selection
            .clone_from(&self.open_type.fs_selection);
        output.open_type_os2_weight_class = self.open_type.weight_class;
        output.open_type_os2_width_class = self.open_type.width_class.map(width_class_to_ufo);
        output
            .open_type_os2_vendor_id
            .clone_from(&self.open_type.vendor_id);
        output.note.clone_from(&self.note);
        output.version_major = self.version_major;
        output.version_minor = self.version_minor;

        if *info == output {
            return Ok(false);
        }
        *info = output;
        Ok(true)
    }
}

/// Remove only canonically owned fields from a UFO preservation template.
pub fn clear_canonical_font_info_fields(info: &mut norad::FontInfo) -> bool {
    CanonicalFontInfo::default()
        .write_to_ufo(info)
        .expect("empty canonical font info is valid")
}

fn validate_metrics(metrics: &CanonicalFontMetrics) -> Result<(), CanonicalFontInfoError> {
    for (name, value) in [
        ("units per em", metrics.units_per_em),
        ("ascender", metrics.ascender),
        ("descender", metrics.descender),
        ("x-height", metrics.x_height),
        ("cap height", metrics.cap_height),
        ("italic angle", metrics.italic_angle),
    ] {
        if value.is_some_and(|value| !value.is_finite()) {
            return Err(CanonicalFontInfoError::NonFinite(name));
        }
    }
    if metrics.units_per_em.is_some_and(|value| value < 0.0) {
        return Err(CanonicalFontInfoError::NegativeUnitsPerEm);
    }
    Ok(())
}

fn width_class_from_ufo(value: norad::fontinfo::Os2WidthClass) -> OpenTypeWidthClass {
    use norad::fontinfo::Os2WidthClass as Ufo;
    match value {
        Ufo::UltraCondensed => OpenTypeWidthClass::UltraCondensed,
        Ufo::ExtraCondensed => OpenTypeWidthClass::ExtraCondensed,
        Ufo::Condensed => OpenTypeWidthClass::Condensed,
        Ufo::SemiCondensed => OpenTypeWidthClass::SemiCondensed,
        Ufo::Normal => OpenTypeWidthClass::Normal,
        Ufo::SemiExpanded => OpenTypeWidthClass::SemiExpanded,
        Ufo::Expanded => OpenTypeWidthClass::Expanded,
        Ufo::ExtraExpanded => OpenTypeWidthClass::ExtraExpanded,
        Ufo::UltraExpanded => OpenTypeWidthClass::UltraExpanded,
    }
}

fn width_class_to_ufo(value: OpenTypeWidthClass) -> norad::fontinfo::Os2WidthClass {
    use norad::fontinfo::Os2WidthClass as Ufo;
    match value {
        OpenTypeWidthClass::UltraCondensed => Ufo::UltraCondensed,
        OpenTypeWidthClass::ExtraCondensed => Ufo::ExtraCondensed,
        OpenTypeWidthClass::Condensed => Ufo::Condensed,
        OpenTypeWidthClass::SemiCondensed => Ufo::SemiCondensed,
        OpenTypeWidthClass::Normal => Ufo::Normal,
        OpenTypeWidthClass::SemiExpanded => Ufo::SemiExpanded,
        OpenTypeWidthClass::Expanded => Ufo::Expanded,
        OpenTypeWidthClass::ExtraExpanded => Ufo::ExtraExpanded,
        OpenTypeWidthClass::UltraExpanded => Ufo::UltraExpanded,
    }
}
