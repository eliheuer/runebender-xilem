// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Font data model wrapping `norad` UFO types for thread-safe access.
//!
//! `Workspace` loads a `.ufo` font file and stores its glyphs, metrics,
//! kerning, and group data in owned Rust structs. It is shared across threads
//! via `Arc<RwLock<Workspace>>`; the `read_workspace` / `write_workspace`
//! helpers at the bottom of this file acquire the lock with poison recovery.
//! Glyphs are sorted by Unicode codepoint for stable grid display order.

use anyhow::{Context, Result};
use kurbo::Affine;
use norad::{Font, Glyph as NoradGlyph};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::entity_id::EntityId;

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
    /// Components referencing other glyphs
    pub components: Vec<Component>,
    /// Left kerning group (e.g., "public.kern1.O")
    pub left_group: Option<String>,
    /// Right kerning group (e.g., "public.kern2.O")
    pub right_group: Option<String>,
    /// Mark color (UFO public.markColor), stored as "R,G,B,A"
    /// with 0–1 floats
    pub mark_color: Option<String>,
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
    /// Hyperbezier smooth point (on-curve, auto control points)
    Hyper,
    /// Hyperbezier corner point (on-curve, independent segments)
    HyperCorner,
}

/// A component reference to another glyph
///
/// Components allow glyphs to reuse other glyphs as building blocks.
/// This is heavily used in Arabic fonts where base letters are combined
/// with dots and diacritical marks.
#[derive(Debug, Clone)]
pub struct Component {
    /// Name of the referenced glyph (the "base" glyph)
    pub base: String,
    /// Affine transformation applied to the component
    /// Default is identity: [1, 0, 0, 1, 0, 0]
    pub transform: Affine,
    /// Unique identifier for selection and hit testing
    pub id: EntityId,
}

impl Component {
    /// Create a component from norad's Component type
    pub fn from_norad(norad_comp: &norad::Component) -> Self {
        // norad's AffineTransform has: x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset
        let t = &norad_comp.transform;
        let transform = Affine::new([
            t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
        ]);

        Self {
            base: norad_comp.base.to_string(),
            transform,
            id: EntityId::next(),
        }
    }

    /// Convert to norad's Component type for saving
    pub fn to_norad(&self) -> norad::Component {
        let coeffs = self.transform.as_coeffs();
        let transform = norad::AffineTransform {
            x_scale: coeffs[0],
            xy_scale: coeffs[1],
            yx_scale: coeffs[2],
            y_scale: coeffs[3],
            x_offset: coeffs[4],
            y_offset: coeffs[5],
        };

        norad::Component::new(
            norad::Name::new(&self.base).expect("Invalid component base name"),
            transform,
            None, // identifier
            None, // lib
        )
    }

    /// Translate the component by a delta
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.transform = Affine::translate((dx, dy)) * self.transform;
    }
}

#[allow(dead_code)]
impl Glyph {
    /// Calculate the left side bearing (LSB)
    /// This is the distance from x=0 to the leftmost point in the glyph
    pub fn left_side_bearing(&self) -> f64 {
        let min_x = self.bounding_box_min_x();
        min_x.unwrap_or(0.0)
    }

    /// Calculate the right side bearing (RSB)
    /// This is the distance from the rightmost point to the advance width
    pub fn right_side_bearing(&self) -> f64 {
        let max_x = self.bounding_box_max_x();
        match max_x {
            Some(max_x) => self.width - max_x,
            None => self.width, // Empty glyph: RSB = width
        }
    }

    /// Get the minimum x coordinate from all contour points
    fn bounding_box_min_x(&self) -> Option<f64> {
        self.contours
            .iter()
            .flat_map(|c| c.points.iter())
            .map(|p| p.x)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
    }

    /// Get the maximum x coordinate from all contour points
    fn bounding_box_max_x(&self) -> Option<f64> {
        self.contours
            .iter()
            .flat_map(|c| c.points.iter())
            .map(|p| p.x)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
    }
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

    /// Kerning pairs: first_member -> (second_member -> kern_value)
    /// First member can be a glyph name or "public.kern1.*" group name
    /// Second member can be a glyph name or "public.kern2.*" group name
    pub kerning: HashMap<String, HashMap<String, f64>>,

    /// Kerning groups: group_name -> [glyph_names]
    /// e.g., "public.kern1.O" -> ["O", "D", "Q"]
    /// Loaded from groups.plist and merged with glyph-level groups
    pub groups: HashMap<String, Vec<String>>,
}

impl Workspace {
    /// Load a UFO from a directory path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Load the UFO using norad
        let font =
            Font::load(path).with_context(|| format!("Failed to load UFO from {:?}", path))?;

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

        // Convert kerning from norad's BTreeMap to HashMap
        // norad's Kerning type: BTreeMap<Name, BTreeMap<Name, f64>>
        let kerning = font
            .kerning
            .iter()
            .map(|(first, second_map)| {
                let inner: HashMap<String, f64> = second_map
                    .iter()
                    .map(|(second, value)| (second.to_string(), *value))
                    .collect();
                (first.to_string(), inner)
            })
            .collect();

        // Convert groups from norad's BTreeMap to HashMap
        // norad's Groups type: BTreeMap<Name, Vec<Name>>
        let groups = font
            .groups
            .iter()
            .map(|(group_name, glyph_names)| {
                let names: Vec<String> = glyph_names.iter().map(|n| n.to_string()).collect();
                (group_name.to_string(), names)
            })
            .collect();

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
            kerning,
            groups,
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

        // Convert components
        let components = norad_glyph
            .components
            .iter()
            .map(Component::from_norad)
            .collect();

        // Extract kerning groups from lib data
        let left_group = norad_glyph
            .lib
            .get("public.kern1")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());
        let right_group = norad_glyph
            .lib
            .get("public.kern2")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());

        // Extract mark color from lib data
        let mark_color = norad_glyph
            .lib
            .get("public.markColor")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());

        Glyph {
            name,
            width,
            height: Some(height),
            codepoints,
            contours,
            components,
            left_group,
            right_group,
            mark_color,
        }
    }

    /// Convert a norad contour to our internal Contour
    fn convert_contour(norad_contour: &norad::Contour) -> Contour {
        // Check if this is a hyperbezier contour using the identifier attribute
        // ONLY use the identifier - don't use heuristics to avoid false positives
        let is_hyperbezier = norad_contour
            .identifier()
            .map(|id| id.as_ref().contains("hyper"))
            .unwrap_or(false);

        let points = norad_contour
            .points
            .iter()
            .map(|pt| Self::convert_point(pt, is_hyperbezier))
            .collect();
        Contour { points }
    }

    /// Convert a norad point to our internal ContourPoint
    fn convert_point(pt: &norad::ContourPoint, is_hyperbezier: bool) -> ContourPoint {
        ContourPoint {
            x: pt.x,
            y: pt.y,
            point_type: if is_hyperbezier {
                // In hyperbezier contours:
                // - type="curve" -> smooth hyperbezier point
                // - type="line" -> corner hyperbezier point
                // - type="move" -> smooth hyperbezier point (first point)
                match pt.typ {
                    norad::PointType::Curve => PointType::Hyper,
                    norad::PointType::Line => PointType::HyperCorner,
                    norad::PointType::Move => PointType::Hyper,
                    _ => Self::convert_point_type(&pt.typ),
                }
            } else {
                Self::convert_point_type(&pt.typ)
            },
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

    /// Get a mutable reference to a glyph by name
    pub fn get_glyph_mut(&mut self, name: &str) -> Option<&mut Glyph> {
        self.glyphs.get_mut(name)
    }

    /// Update a glyph in the workspace
    pub fn update_glyph(&mut self, glyph_name: &str, glyph: Glyph) {
        self.glyphs.insert(glyph_name.to_string(), glyph);
    }

    /// Save the UFO back to disk
    pub fn save(&self) -> Result<()> {
        // Load the original font to preserve metadata we don't edit
        let mut font = Font::load(&self.path)
            .with_context(|| format!("Failed to load UFO for saving: {:?}", self.path))?;

        // Update all glyphs in the default layer
        let default_layer = font.default_layer_mut();

        for (name, glyph) in &self.glyphs {
            let norad_glyph = Self::to_norad_glyph(glyph);

            // Remove old glyph and insert updated one
            if default_layer.contains_glyph(name) {
                default_layer.remove_glyph(name);
            }
            default_layer.insert_glyph(norad_glyph);
        }

        // Update kerning data
        // Convert from HashMap<String, HashMap<String, f64>> to BTreeMap<Name, BTreeMap<Name, f64>>
        font.kerning.clear();
        for (first, second_map) in &self.kerning {
            let first_name = norad::Name::new(first).ok();
            if let Some(first_name) = first_name {
                let mut inner = std::collections::BTreeMap::new();
                for (second, value) in second_map {
                    if let Ok(second_name) = norad::Name::new(second) {
                        inner.insert(second_name, *value);
                    }
                }
                font.kerning.insert(first_name, inner);
            }
        }

        // Update groups data
        // Convert from HashMap<String, Vec<String>> to BTreeMap<Name, Vec<Name>>
        font.groups.clear();
        for (group_name, glyph_names) in &self.groups {
            let group_name_obj = norad::Name::new(group_name).ok();
            if let Some(group_name_obj) = group_name_obj {
                let names: Vec<norad::Name> = glyph_names
                    .iter()
                    .filter_map(|n| norad::Name::new(n).ok())
                    .collect();
                font.groups.insert(group_name_obj, names);
            }
        }

        // Save back to disk
        font.save(&self.path)
            .with_context(|| format!("Failed to save UFO to {:?}", self.path))?;

        Ok(())
    }

    /// Convert our internal Glyph to norad Glyph
    fn to_norad_glyph(glyph: &Glyph) -> NoradGlyph {
        let mut norad_glyph = NoradGlyph::new(&glyph.name);
        norad_glyph.width = glyph.width;
        if let Some(height) = glyph.height {
            norad_glyph.height = height;
        }

        // Convert codepoints
        for &cp in &glyph.codepoints {
            norad_glyph.codepoints.insert(cp);
        }

        // Convert contours
        norad_glyph.contours = glyph.contours.iter().map(Self::to_norad_contour).collect();

        // Convert components
        norad_glyph.components = glyph.components.iter().map(Component::to_norad).collect();

        // Save kerning groups to lib data
        if let Some(left_group) = &glyph.left_group {
            norad_glyph
                .lib
                .insert("public.kern1".to_string(), left_group.clone().into());
        }
        if let Some(right_group) = &glyph.right_group {
            norad_glyph
                .lib
                .insert("public.kern2".to_string(), right_group.clone().into());
        }

        // Save mark color to lib data
        if let Some(mark_color) = &glyph.mark_color {
            norad_glyph
                .lib
                .insert("public.markColor".to_string(), mark_color.clone().into());
        }

        norad_glyph
    }

    /// Convert our internal Contour to norad Contour
    fn to_norad_contour(contour: &Contour) -> norad::Contour {
        let points = contour.points.iter().map(Self::to_norad_point).collect();

        // Check if this is a hyperbezier contour
        let is_hyperbezier = contour
            .points
            .iter()
            .any(|pt| matches!(pt.point_type, PointType::Hyper | PointType::HyperCorner));

        // Set identifier="hyperbezier" for hyperbezier contours
        let identifier = if is_hyperbezier {
            Some(norad::Identifier::new("hyperbezier").unwrap())
        } else {
            None
        };

        norad::Contour::new(points, identifier, None)
    }

    /// Convert our internal ContourPoint to norad ContourPoint
    fn to_norad_point(pt: &ContourPoint) -> norad::ContourPoint {
        let is_hyper = matches!(pt.point_type, PointType::Hyper | PointType::HyperCorner);

        // For hyperbezier points:
        // - Round coordinates to integers
        // - Don't set smooth attribute (not needed for detection)
        let (x, y) = if is_hyper {
            (pt.x.round(), pt.y.round())
        } else {
            (pt.x, pt.y)
        };

        norad::ContourPoint::new(
            x,
            y,
            Self::to_norad_point_type(pt.point_type),
            false, // smooth - don't set for hyperbeziers
            None,  // name
            None,  // identifier
            None,  // lib (plist dictionary)
        )
    }

    /// Convert our internal PointType to norad PointType
    fn to_norad_point_type(typ: PointType) -> norad::PointType {
        match typ {
            PointType::Move => norad::PointType::Move,
            PointType::Line => norad::PointType::Line,
            PointType::OffCurve => norad::PointType::OffCurve,
            PointType::Curve => norad::PointType::Curve,
            PointType::QCurve => norad::PointType::QCurve,
            // Hyperbezier smooth points stored as type="curve"
            PointType::Hyper => norad::PointType::Curve,
            // Hyperbezier corner points stored as type="line"
            PointType::HyperCorner => norad::PointType::Line,
        }
    }
}

// ============================================================================
// RWLOCK HELPERS
// ============================================================================

/// Acquire a read lock on a shared workspace, recovering from poison.
///
/// If the lock is poisoned (a thread panicked while holding it), this
/// recovers the inner data instead of panicking. This keeps the app
/// running even after an unexpected failure in another thread.
pub fn read_workspace(ws: &Arc<RwLock<Workspace>>) -> RwLockReadGuard<'_, Workspace> {
    ws.read().unwrap_or_else(|poisoned| {
        tracing::warn!("Workspace RwLock was poisoned, recovering");
        poisoned.into_inner()
    })
}

/// Acquire a write lock on a shared workspace, recovering from poison.
///
/// See [`read_workspace`] for details on poison recovery.
pub fn write_workspace(ws: &Arc<RwLock<Workspace>>) -> RwLockWriteGuard<'_, Workspace> {
    ws.write().unwrap_or_else(|poisoned| {
        tracing::warn!("Workspace RwLock was poisoned, recovering");
        poisoned.into_inner()
    })
}
