// Ported from runebender-xilem/src/path/point_list.rs (Apache-2.0).

//! Point collection for bezier paths.

use super::point::PathPoint;
use std::sync::Arc;

/// A collection of points in a bezier path.
///
/// Uses `Arc` for cheap cloning while maintaining shared data.
#[derive(Debug, Clone)]
pub struct PathPoints {
    points: Arc<Vec<PathPoint>>,
}

impl PathPoints {
    /// Creates an empty point list.
    pub fn new() -> Self {
        Self {
            points: Arc::new(Vec::new()),
        }
    }

    /// Wraps an existing vector without copying it.
    pub fn from_vec(points: Vec<PathPoint>) -> Self {
        Self {
            points: Arc::new(points),
        }
    }

    /// Returns the number of points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns `true` when the list holds no points.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Iterates over the points in path order.
    pub fn iter(&self) -> impl Iterator<Item = &PathPoint> {
        self.points.iter()
    }

    /// Returns the points as a slice.
    pub fn as_slice(&self) -> &[PathPoint] {
        self.points.as_slice()
    }

    /// Get mutable access to the points. Clones the data if the `Arc`
    /// has multiple references.
    pub fn make_mut(&mut self) -> &mut Vec<PathPoint> {
        Arc::make_mut(&mut self.points)
    }

    /// Convert to a vector. Clones the data if the `Arc` has multiple
    /// references.
    pub fn to_vec(&self) -> Vec<PathPoint> {
        (*self.points).clone()
    }
}

impl Default for PathPoints {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<PathPoint>> for PathPoints {
    fn from(points: Vec<PathPoint>) -> Self {
        Self::from_vec(points)
    }
}
