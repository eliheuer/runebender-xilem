// Path representation & geometry. Ported from
// runebender-xilem/src/path/.

//! Path abstraction for glyph outlines: the editable representation.
//!
//! The `Path` enum wraps three curve types: `Cubic` (standard UFO
//! beziers), `Quadratic` (TrueType-style), and `Hyper` (hyperbezier
//! splines with only on-curve points). All three convert to
//! `kurbo::BezPath` for rendering. Paths are created from
//! `workspace::Contour` data when a glyph is opened for editing, and
//! converted back when the session is saved.

pub mod cubic;
pub mod hyper;
pub mod hyper_model;
pub mod point;
pub mod point_list;
pub mod quadrant;
pub mod quadratic;
pub mod segment;

pub use cubic::CubicPath;
pub use hyper::HyperPath;
pub use point::{PathPoint, PointType};
pub use point_list::PathPoints;
pub use quadrant::Quadrant;
pub use quadratic::QuadraticPath;
pub use segment::{Segment, SegmentInfo};

use self::hyper_model as workspace;
use kurbo::BezPath;

/// A path in a glyph outline. Supports cubic, quadratic, and
/// hyperbezier paths.
#[derive(Debug, Clone)]
pub enum Path {
    /// A contour with explicit cubic control points (UFO `curve` segments).
    Cubic(CubicPath),
    /// A contour with quadratic control points (UFO `qcurve` segments).
    Quadratic(QuadraticPath),
    /// A hyperbezier contour; only on-curve points are stored and handles are solved.
    Hyper(HyperPath),
}

impl Path {
    /// Renders this path as a new `kurbo::BezPath`.
    pub fn to_bezpath(&self) -> BezPath {
        match self {
            Path::Cubic(cubic) => cubic.to_bezpath(),
            Path::Quadratic(quadratic) => quadratic.to_bezpath(),
            Path::Hyper(hyper) => hyper.to_bezpath(),
        }
    }

    /// Appends this path's elements to an existing `BezPath` without clearing it.
    pub fn append_to_bezpath(&self, path: &mut BezPath) {
        match self {
            Path::Cubic(cubic) => cubic.append_to_bezpath(path),
            Path::Quadratic(quadratic) => quadratic.append_to_bezpath(path),
            Path::Hyper(hyper) => hyper.append_to_bezpath(path),
        }
    }

    /// Detect the curve type from the contour and dispatch.
    pub fn from_contour(contour: &workspace::Contour) -> Self {
        let has_hyper = contour.points.iter().any(|pt| {
            matches!(
                pt.point_type,
                workspace::PointType::Hyper | workspace::PointType::HyperCorner
            )
        });

        if has_hyper {
            return Path::Hyper(HyperPath::from_contour(contour));
        }

        let has_qcurve = contour
            .points
            .iter()
            .any(|pt| matches!(pt.point_type, workspace::PointType::QCurve));

        if has_qcurve {
            Path::Quadratic(QuadraticPath::from_contour(contour))
        } else {
            Path::Cubic(CubicPath::from_contour(contour))
        }
    }

    /// Returns the path's editable points, whatever the curve type.
    pub fn points(&self) -> &PathPoints {
        match self {
            Path::Cubic(cubic) => cubic.points(),
            Path::Quadratic(quadratic) => quadratic.points(),
            Path::Hyper(hyper) => hyper.points(),
        }
    }

    /// Converts back to the `workspace::Contour` form used for saving and conversion.
    pub fn to_contour(&self) -> workspace::Contour {
        match self {
            Path::Cubic(cubic) => cubic.to_contour(),
            Path::Quadratic(quadratic) => quadratic.to_contour(),
            Path::Hyper(hyper) => hyper.to_contour(),
        }
    }
}
