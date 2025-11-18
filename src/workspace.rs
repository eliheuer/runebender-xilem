// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Font workspace management - handles UFO loading and glyph access

use anyhow::{Context, Result};
use norad::{Font, Glyph as NoradGlyph};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Internal representation of a glyph (thread-safe, owned data)
#[derive(Debug, Clone)]
pub struct Glyph {
    pub name: String,
    pub width: f64,
    pub height: Option<f64>,
    pub codepoints: Vec<char>,
    pub contours: Vec<Contour>,
}

/// A contour is a closed path
#[derive(Debug, Clone)]
pub struct Contour {
    pub points: Vec<ContourPoint>,
}

/// A point in a contour
#[derive(Debug, Clone)]
pub struct ContourPoint {
    pub x: f64,
    pub y: f64,
    pub point_type: PointType,
}

/// Point type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointType {
    Move,
    Line,
    OffCurve,
    Curve,
    QCurve,
}

// ============================================================================
// WORKSPACE
// ============================================================================

/// A workspace represents a loaded UFO font with all its glyphs and
/// metadata
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Path to the UFO directory
    pub path: PathBuf,

    /// Name of the font family
    pub family_name: String,

    /// Style name (e.g., "Regular", "Bold")
    pub style_name: String,

    /// All glyphs, indexed by name
    pub glyphs: HashMap<String, Glyph>,

    /// Font metrics
    pub units_per_em: Option<f64>,
    pub ascender: Option<f64>,
    pub descender: Option<f64>,
    pub x_height: Option<f64>,
    pub cap_height: Option<f64>,
}

impl Workspace {
    /// Load a UFO from a directory path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Load the UFO using norad
        let font = Font::load(path)
            .with_context(|| format!("Failed to load UFO from {:?}", path))?;

        // Extract font metadata
        let family_name = font
            .font_info
            .family_name
            .clone()
            .unwrap_or_else(|| "Untitled Font".to_string());

        let style_name = font
            .font_info
            .style_name
            .clone()
            .unwrap_or_else(|| "Regular".to_string());

        // Convert all glyphs to our internal format
        let mut glyphs = HashMap::new();
        for norad_glyph in font.default_layer().iter() {
            let glyph = Self::convert_glyph(norad_glyph);
            glyphs.insert(glyph.name.clone(), glyph);
        }

        Ok(Self {
            path: path.to_path_buf(),
            family_name,
            style_name,
            glyphs,
            units_per_em: font.font_info.units_per_em.map(|n| n.as_f64()),
            ascender: font.font_info.ascender,
            descender: font.font_info.descender,
            x_height: font.font_info.x_height,
            cap_height: font.font_info.cap_height,
        })
    }

    /// Convert a norad Glyph to our internal Glyph
    fn convert_glyph(norad_glyph: &NoradGlyph) -> Glyph {
        let name = norad_glyph.name().to_string();
        let width = norad_glyph.width;
        let height = norad_glyph.height;

        // Convert codepoints from norad's Codepoints type
        let codepoints: Vec<char> = norad_glyph.codepoints.iter().collect();

        // Convert contours
        let contours = norad_glyph
            .contours
            .iter()
            .map(Self::convert_contour)
            .collect();

        Glyph {
            name,
            width,
            height: Some(height),
            codepoints,
            contours,
        }
    }

    /// Convert a norad contour to our internal Contour
    fn convert_contour(norad_contour: &norad::Contour) -> Contour {
        let points = norad_contour
            .points
            .iter()
            .map(Self::convert_point)
            .collect();
        Contour { points }
    }

    /// Convert a norad point to our internal ContourPoint
    fn convert_point(pt: &norad::ContourPoint) -> ContourPoint {
        ContourPoint {
            x: pt.x,
            y: pt.y,
            point_type: Self::convert_point_type(&pt.typ),
        }
    }

    /// Convert a norad PointType to our internal PointType
    fn convert_point_type(typ: &norad::PointType) -> PointType {
        match typ {
            norad::PointType::Move => PointType::Move,
            norad::PointType::Line => PointType::Line,
            norad::PointType::OffCurve => PointType::OffCurve,
            norad::PointType::Curve => PointType::Curve,
            norad::PointType::QCurve => PointType::QCurve,
        }
    }

    /// Get the display name of the font (Family + Style)
    pub fn display_name(&self) -> String {
        format!("{} {}", self.family_name, self.style_name)
    }

    /// Get the number of glyphs
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Get a list of all glyph names, sorted by Unicode codepoint
    pub fn glyph_names(&self) -> Vec<String> {
        let mut glyph_list: Vec<_> = self.glyphs.iter().collect();

        glyph_list.sort_by(|(name_a, glyph_a), (name_b, glyph_b)| {
            Self::compare_glyphs(name_a, glyph_a, name_b, glyph_b)
        });

        glyph_list
            .into_iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Compare two glyphs for sorting
    fn compare_glyphs(
        name_a: &str,
        glyph_a: &Glyph,
        name_b: &str,
        glyph_b: &Glyph,
    ) -> std::cmp::Ordering {
        let cp_a = glyph_a.codepoints.first();
        let cp_b = glyph_b.codepoints.first();

        match (cp_a, cp_b) {
            // Both have codepoints: compare by codepoint value
            (Some(a), Some(b)) => a.cmp(b),
            // Only a has codepoint: a comes first
            (Some(_), None) => std::cmp::Ordering::Less,
            // Only b has codepoint: b comes first
            (None, Some(_)) => std::cmp::Ordering::Greater,
            // Neither has codepoint: compare by name alphabetically
            (None, None) => name_a.cmp(name_b),
        }
    }

    /// Get a glyph by name
    pub fn get_glyph(&self, name: &str) -> Option<&Glyph> {
        self.glyphs.get(name)
    }

    /// Update a glyph in the workspace
    pub fn update_glyph(&mut self, glyph_name: &str, glyph: Glyph) {
        self.glyphs.insert(glyph_name.to_string(), glyph);
    }

    /// Save the UFO back to disk
    ///
    /// TODO: This needs to convert our internal data back to norad format
    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        // For now, just a placeholder
        // We'd need to convert our data back to norad types and save
        anyhow::bail!("Save not yet implemented")
    }
}
