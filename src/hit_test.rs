// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Hit testing utilities for finding entities under the cursor

use crate::entity_id::EntityId;
use kurbo::Point;

/// Default maximum distance for clicking on a point (in screen pixels)
pub const MIN_CLICK_DISTANCE: f64 = 10.0;

/// Maximum distance for clicking on a segment (stricter)
#[allow(dead_code)]
pub const SEGMENT_CLICK_DISTANCE: f64 = 6.0;

/// Penalty added to on-curve points to favor selecting off-curve points
/// This makes it easier to grab handles when they're near on-curve points
pub const ON_CURVE_PENALTY: f64 = 5.0;

/// Result of a hit test
#[derive(Debug, Clone, Copy)]
pub struct HitTestResult {
    /// The entity that was hit
    pub entity: EntityId,
    /// Distance from the test point
    pub distance: f64,
}

/// Find the closest entity to a point
///
/// Returns the entity and distance if found within max_dist
pub fn find_closest(
    point: Point,
    candidates: impl Iterator<Item = (EntityId, Point, bool)>,
    max_dist: f64,
) -> Option<HitTestResult> {
    let mut best: Option<HitTestResult> = None;
    let mut best_score = f64::MAX;

    for (entity, candidate_pos, is_on_curve) in candidates {
        let dx = point.x - candidate_pos.x;
        let dy = point.y - candidate_pos.y;
        let distance = (dx * dx + dy * dy).sqrt();

        // Apply penalty to on-curve points to favor off-curve selection
        let score = if is_on_curve {
            distance + ON_CURVE_PENALTY
        } else {
            distance
        };

        if distance <= max_dist && score < best_score {
            best_score = score;
            best = Some(HitTestResult { entity, distance });
        }
    }
    best
}
