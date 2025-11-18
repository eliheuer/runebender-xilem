// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Point collection for bezier paths

use crate::entity_id::EntityId;
use crate::point::PathPoint;
use std::sync::Arc;

/// A collection of points in a bezier path
///
/// Uses Arc for efficient cloning while maintaining shared data.
#[derive(Debug, Clone)]
pub struct PathPoints {
    /// The points in this collection
    points: Arc<Vec<PathPoint>>,
}

impl PathPoints {
    /// Create a new empty point collection
    pub fn new() -> Self {
        Self {
            points: Arc::new(Vec::new()),
        }
    }

    /// Create a point collection from a vector of points
    pub fn from_vec(points: Vec<PathPoint>) -> Self {
        Self {
            points: Arc::new(points),
        }
    }

    /// Get the number of points
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Check if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Get a point by index
    #[allow(dead_code)]
    pub fn get(&self, index: usize) -> Option<&PathPoint> {
        self.points.get(index)
    }

    /// Iterate over all points
    pub fn iter(&self) -> impl Iterator<Item = &PathPoint> {
        self.points.iter()
    }

    /// Find a point by its entity ID
    ///
    /// Returns the index and a reference to the point if found.
    #[allow(dead_code)]
    pub fn find_by_id(
        &self,
        id: EntityId,
    ) -> Option<(usize, &PathPoint)> {
        self.points
            .iter()
            .enumerate()
            .find(|(_, pt)| pt.id == id)
    }

    /// Get mutable access to the points
    ///
    /// This will clone the data if the Arc has multiple references.
    pub fn make_mut(&mut self) -> &mut Vec<PathPoint> {
        Arc::make_mut(&mut self.points)
    }

    /// Convert to a vector
    ///
    /// This clones the data if the Arc has multiple references.
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
