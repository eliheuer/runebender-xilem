// Ported from runebender-xilem/src/model/workspace.rs (Apache-2.0).
//
// Strips: anyhow/norad/file I/O, Arc<RwLock> wrappers, the
// `read_workspace` / `write_workspace` lock helpers, and Workspace::path.
// UFO load/save will be provided by the JS host (via fetch / File API)
// and round-tripped as serialized JSON across the wasm-bindgen boundary.

//! Font data model — owned, single-threaded data structures.

use kurbo::Affine;
use std::collections::HashMap;

use super::entity_id::EntityId;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Internal representation of a glyph (owned data).
#[derive(Debug, Clone)]
pub struct Glyph {
    pub name: String,
    pub width: f64,
    pub height: Option<f64>,
    pub codepoints: Vec<char>,
    pub contours: Vec<Contour>,
    /// Components referencing other glyphs.
    pub components: Vec<Component>,
    /// Left-edge kerning group (e.g., "public.kern2.O").
    pub left_group: Option<String>,
    /// Right-edge kerning group (e.g., "public.kern1.O").
    pub right_group: Option<String>,
    /// Mark color (UFO public.markColor), stored as "R,G,B,A" with 0–1
    /// floats.
    pub mark_color: Option<String>,
}

/// A contour is a closed path.
#[derive(Debug, Clone)]
pub struct Contour {
    pub points: Vec<ContourPoint>,
}

/// A point in a contour.
#[derive(Debug, Clone)]
pub struct ContourPoint {
    pub x: f64,
    pub y: f64,
    pub point_type: PointType,
    /// UFO smooth attribute — tangent continuity.
    pub smooth: bool,
}

/// Point type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointType {
    Move,
    Line,
    OffCurve,
    Curve,
    QCurve,
    /// Hyperbezier smooth point (on-curve, auto control points).
    Hyper,
    /// Hyperbezier corner point (on-curve, independent segments).
    HyperCorner,
}

/// A component reference to another glyph.
///
/// Components allow glyphs to reuse other glyphs as building blocks.
/// This is heavily used in Arabic fonts where base letters are
/// combined with dots and diacritical marks.
#[derive(Debug, Clone)]
pub struct Component {
    /// Name of the referenced glyph (the "base" glyph).
    pub base: String,
    /// Affine transformation applied to the component. Default is
    /// identity: [1, 0, 0, 1, 0, 0].
    pub transform: Affine,
    /// Unique identifier for selection and hit testing.
    pub id: EntityId,
}

impl Component {
    /// Translate the component by a delta.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.transform = Affine::translate((dx, dy)) * self.transform;
    }
}

#[allow(dead_code)]
impl Glyph {
    /// Calculate the left side bearing (LSB). This is the distance
    /// from x=0 to the leftmost point in the glyph.
    pub fn left_side_bearing(&self) -> f64 {
        self.bounding_box_min_x().unwrap_or(0.0)
    }

    /// Calculate the right side bearing (RSB). This is the distance
    /// from the rightmost point to the advance width.
    pub fn right_side_bearing(&self) -> f64 {
        match self.bounding_box_max_x() {
            Some(max_x) => self.width - max_x,
            None => self.width, // Empty glyph: RSB = width
        }
    }

    fn bounding_box_min_x(&self) -> Option<f64> {
        self.contours
            .iter()
            .flat_map(|c| c.points.iter())
            .map(|p| p.x)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
    }

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

/// A workspace represents a loaded font with all its glyphs and
/// metadata. UFO file I/O lives in the JS host; this struct is the
/// in-memory shape that crosses the wasm-bindgen boundary.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub family_name: String,
    pub style_name: String,

    /// All glyphs, indexed by name.
    pub glyphs: HashMap<String, Glyph>,

    pub units_per_em: Option<f64>,
    pub ascender: Option<f64>,
    pub descender: Option<f64>,
    pub x_height: Option<f64>,
    pub cap_height: Option<f64>,

    /// Kerning pairs: `first_member -> (second_member -> kern_value)`.
    /// First member can be a glyph name or "public.kern1.*" group
    /// name; second can be a glyph name or "public.kern2.*" group.
    pub kerning: HashMap<String, HashMap<String, f64>>,

    /// Kerning groups: `group_name -> [glyph_names]` (e.g.,
    /// "public.kern1.O" -> ["O", "D", "Q"]).
    pub groups: HashMap<String, Vec<String>>,
}

impl Workspace {
    /// Create an empty workspace.
    pub fn new(family_name: String, style_name: String) -> Self {
        Self {
            family_name,
            style_name,
            glyphs: HashMap::new(),
            units_per_em: None,
            ascender: None,
            descender: None,
            x_height: None,
            cap_height: None,
            kerning: HashMap::new(),
            groups: HashMap::new(),
        }
    }

    /// Look up a glyph by name.
    pub fn get_glyph(&self, name: &str) -> Option<&Glyph> {
        self.glyphs.get(name)
    }

    /// Glyph names sorted by codepoint (then by name for unencoded).
    pub fn glyph_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.glyphs.keys().map(|s| s.as_str()).collect();
        names.sort_by(|a, b| {
            let ga = self.glyphs.get(*a).and_then(|g| g.codepoints.first());
            let gb = self.glyphs.get(*b).and_then(|g| g.codepoints.first());
            match (ga, gb) {
                (Some(ca), Some(cb)) => ca.cmp(cb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b),
            }
        });
        names
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new(String::new(), String::new())
    }
}
