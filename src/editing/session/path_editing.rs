// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Path editing methods for EditSession — point manipulation,
//! deletion, contour operations, and glyph conversion

use super::EditSession;
use crate::editing::selection::Selection;
use crate::model::workspace::Glyph;
use crate::model::write_workspace;
use crate::path::{HyperPath, Path};
use kurbo::Point;
use std::sync::Arc;

/// Snap a point to the nearest grid position
pub fn snap_point_to_grid(point: Point) -> Point {
    use crate::settings;
    if !settings::snap::ENABLED {
        return point;
    }
    let spacing = settings::snap::SPACING;
    if spacing <= 0.0 {
        return point;
    }
    Point::new(
        (point.x / spacing).round() * spacing,
        (point.y / spacing).round() * spacing,
    )
}

impl EditSession {
    /// Move selected points by a delta in design space
    ///
    /// This mutates the paths using Arc::make_mut, which will clone
    /// the path data if there are other references to it.
    ///
    /// When moving on-curve points, their adjacent off-curve control
    /// points (handles) are also moved to maintain curve shape. This
    /// is standard font editor behavior.
    pub fn move_selection(&mut self, delta: kurbo::Vec2) {
        if self.selection.is_empty() {
            return;
        }

        use crate::model::EntityId;
        use std::collections::HashSet;

        // We need to mutate paths, so convert Arc<Vec<Path>> to
        // mutable Vec
        let paths_vec = Arc::make_mut(&mut self.paths);

        // Build a set of IDs to move:
        // - All selected points
        // - Adjacent off-curve points of selected on-curve points
        let mut points_to_move: HashSet<EntityId> = self.selection.iter().copied().collect();

        // First pass: identify adjacent off-curve points of selected
        // on-curve points
        Self::collect_adjacent_off_curve_points(paths_vec, &self.selection, &mut points_to_move);

        // Second pass: move all identified points
        Self::apply_point_movement(paths_vec, &points_to_move, delta);

        // Third pass: enforce smooth constraints — when a handle
        // adjacent to a smooth on-curve point is dragged, rotate the
        // opposite handle to maintain collinearity
        Self::enforce_smooth_constraints(paths_vec, &points_to_move);
    }

    /// Move only the explicitly selected points, without dragging
    /// adjacent off-curve handles along. This is the "independent
    /// move" mode activated by holding Option/Alt during a drag.
    pub fn move_selection_independent(
        &mut self,
        delta: kurbo::Vec2,
    ) {
        if self.selection.is_empty() {
            return;
        }

        let paths_vec = Arc::make_mut(&mut self.paths);

        // Move only the selected points — skip
        // collect_adjacent_off_curve_points entirely
        let points_to_move: std::collections::HashSet<_> =
            self.selection.iter().copied().collect();

        Self::apply_point_movement(
            paths_vec,
            &points_to_move,
            delta,
        );

        // Still enforce smooth constraints so tangent handles
        // stay collinear where applicable
        Self::enforce_smooth_constraints(
            paths_vec,
            &points_to_move,
        );
    }

    /// Snap selected on-curve points to the nearest design grid line.
    ///
    /// Off-curve handles are shifted by the same amount as their
    /// parent on-curve point so the curve shape is preserved. Points
    /// that are not selected (and were not moved) are left untouched.
    pub fn snap_selection_to_grid(&mut self) {
        use crate::settings;
        use std::collections::HashMap;

        if !settings::snap::ENABLED || self.selection.is_empty() {
            return;
        }

        let spacing = settings::snap::SPACING;
        if spacing <= 0.0 {
            return;
        }

        let paths_vec = Arc::make_mut(&mut self.paths);

        // First pass: compute snap offsets for selected on-curve
        // points and record which off-curve neighbors to shift.
        let mut snap_offsets: HashMap<crate::model::EntityId, kurbo::Vec2> =
            HashMap::new();

        for path in paths_vec.iter() {
            let (points_slice, closed) = match path {
                Path::Cubic(c) => {
                    let v: Vec<_> = c.points.iter().collect();
                    (v, c.closed)
                }
                Path::Quadratic(q) => {
                    let v: Vec<_> = q.points.iter().collect();
                    (v, q.closed)
                }
                Path::Hyper(h) => {
                    let v: Vec<_> = h.points.iter().collect();
                    (v, h.closed)
                }
            };

            let len = points_slice.len();
            for (i, pt) in points_slice.iter().enumerate() {
                if !self.selection.contains(&pt.id) {
                    continue;
                }

                if !pt.is_on_curve() {
                    continue;
                }

                // Snap on-curve points to grid
                let snapped_x =
                    (pt.point.x / spacing).round() * spacing;
                let snapped_y =
                    (pt.point.y / spacing).round() * spacing;
                let offset = kurbo::Vec2::new(
                    snapped_x - pt.point.x,
                    snapped_y - pt.point.y,
                );

                if offset.x.abs() < 1e-9 && offset.y.abs() < 1e-9 {
                    continue; // Already on grid
                }

                snap_offsets.insert(pt.id, offset);

                // Shift adjacent off-curve handles by the same
                // amount
                if let Some(prev) =
                    Self::get_previous_index(i, len, closed)
                    && points_slice[prev].is_off_curve()
                {
                    snap_offsets
                        .entry(points_slice[prev].id)
                        .or_insert(offset);
                }
                if let Some(next) =
                    Self::get_next_index(i, len, closed)
                    && points_slice[next].is_off_curve()
                {
                    snap_offsets
                        .entry(points_slice[next].id)
                        .or_insert(offset);
                }
            }
        }

        if snap_offsets.is_empty() {
            return;
        }

        // Second pass: apply the offsets
        for path in paths_vec.iter_mut() {
            match path {
                Path::Cubic(c) => {
                    for pt in c.points.make_mut().iter_mut() {
                        if let Some(off) = snap_offsets.get(&pt.id) {
                            pt.point = Point::new(
                                pt.point.x + off.x,
                                pt.point.y + off.y,
                            );
                        }
                    }
                }
                Path::Quadratic(q) => {
                    for pt in q.points.make_mut().iter_mut() {
                        if let Some(off) = snap_offsets.get(&pt.id) {
                            pt.point = Point::new(
                                pt.point.x + off.x,
                                pt.point.y + off.y,
                            );
                        }
                    }
                }
                Path::Hyper(h) => {
                    let mut changed = false;
                    for pt in h.points.make_mut().iter_mut() {
                        if let Some(off) = snap_offsets.get(&pt.id) {
                            pt.point = Point::new(
                                pt.point.x + off.x,
                                pt.point.y + off.y,
                            );
                            changed = true;
                        }
                    }
                    if changed {
                        h.after_change();
                    }
                }
            }
        }
    }

    /// Nudge selected points in a direction
    ///
    /// Nudge amounts are configured in `settings::nudge`.
    pub fn nudge_selection(&mut self, dx: f64, dy: f64, shift: bool, ctrl: bool) {
        use crate::settings;

        let amount = if ctrl {
            settings::nudge::CMD
        } else if shift {
            settings::nudge::SHIFT
        } else {
            settings::nudge::BASE
        };

        let delta = kurbo::Vec2::new(dx * amount, dy * amount);
        self.move_selection(delta);
    }

    // ================================================================
    // SELECTION BOUNDING BOX
    // ================================================================

    /// Compute the bounding box of all selected points
    ///
    /// Returns None if nothing is selected.
    pub fn selection_bounding_box(&self) -> Option<kurbo::Rect> {
        if self.selection.is_empty() {
            return None;
        }

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut found = false;

        for path in self.paths.iter() {
            let points = match path {
                Path::Cubic(c) => c.points(),
                Path::Quadratic(q) => q.points(),
                Path::Hyper(h) => h.points(),
            };
            for pt in points.iter() {
                if self.selection.contains(&pt.id) {
                    min_x = min_x.min(pt.point.x);
                    min_y = min_y.min(pt.point.y);
                    max_x = max_x.max(pt.point.x);
                    max_y = max_y.max(pt.point.y);
                    found = true;
                }
            }
        }

        if found {
            Some(kurbo::Rect::new(min_x, min_y, max_x, max_y))
        } else {
            None
        }
    }

    // ================================================================
    // TRANSFORM SELECTION
    // ================================================================

    /// Apply an affine transform to all selected points
    ///
    /// Adjacent off-curve handles of selected on-curve points are
    /// also transformed. After the transform, points are snapped to
    /// grid and smooth constraints are enforced.
    pub fn transform_selection(&mut self, affine: kurbo::Affine) {
        if self.selection.is_empty() {
            return;
        }

        use crate::model::EntityId;
        use std::collections::HashSet;

        let paths_vec = Arc::make_mut(&mut self.paths);

        // Build set of points to transform (selected + adjacent
        // off-curve handles of selected on-curve points)
        let mut points_to_transform: HashSet<EntityId> =
            self.selection.iter().copied().collect();

        Self::collect_adjacent_off_curve_points(
            paths_vec,
            &self.selection,
            &mut points_to_transform,
        );

        // Apply the affine transform to each point
        for path in paths_vec.iter_mut() {
            match path {
                Path::Cubic(cubic) => {
                    for pt in cubic.points.make_mut().iter_mut() {
                        if points_to_transform.contains(&pt.id) {
                            pt.point = affine * pt.point;
                        }
                    }
                }
                Path::Quadratic(quadratic) => {
                    for pt in quadratic.points.make_mut().iter_mut()
                    {
                        if points_to_transform.contains(&pt.id) {
                            pt.point = affine * pt.point;
                        }
                    }
                }
                Path::Hyper(hyper) => {
                    let mut changed = false;
                    for pt in hyper.points.make_mut().iter_mut() {
                        if points_to_transform.contains(&pt.id) {
                            pt.point = affine * pt.point;
                            changed = true;
                        }
                    }
                    if changed {
                        hyper.after_change();
                    }
                }
            }
        }

        // If the transform is a reflection (negative determinant),
        // reverse affected contours to maintain winding direction
        if affine.determinant() < 0.0 {
            Self::reverse_affected_contours(
                paths_vec,
                &points_to_transform,
            );
        }

        // Enforce smooth constraints with all transformed points
        // as "disturbed"
        Self::enforce_smooth_constraints(
            paths_vec,
            &points_to_transform,
        );
    }

    /// Flip selected points horizontally around the selection center
    pub fn flip_selection_horizontal(&mut self) {
        let center = match self.selection_bounding_box() {
            Some(rect) => rect.center(),
            None => return,
        };

        let affine = kurbo::Affine::translate(center.to_vec2())
            .then_scale_non_uniform(-1.0, 1.0)
            .then_translate(-center.to_vec2());

        self.transform_selection(affine);
        self.last_transform = Some(affine);
    }

    /// Flip selected points vertically around the selection center
    pub fn flip_selection_vertical(&mut self) {
        let center = match self.selection_bounding_box() {
            Some(rect) => rect.center(),
            None => return,
        };

        let affine = kurbo::Affine::translate(center.to_vec2())
            .then_scale_non_uniform(1.0, -1.0)
            .then_translate(-center.to_vec2());

        self.transform_selection(affine);
        self.last_transform = Some(affine);
    }

    /// Rotate selected points by the given angle (in degrees)
    /// around the selection center
    pub fn rotate_selection(&mut self, degrees: f64) {
        let center = match self.selection_bounding_box() {
            Some(rect) => rect.center(),
            None => return,
        };

        let radians = degrees * std::f64::consts::PI / 180.0;
        let affine = kurbo::Affine::rotate_about(radians, center);

        self.transform_selection(affine);
        self.last_transform = Some(affine);
    }

    /// Scale selected points by (sx, sy) around the selection center
    #[allow(dead_code)]
    pub fn scale_selection(&mut self, sx: f64, sy: f64) {
        let center = match self.selection_bounding_box() {
            Some(rect) => rect.center(),
            None => return,
        };

        let affine = kurbo::Affine::translate(center.to_vec2())
            .then_scale_non_uniform(sx, sy)
            .then_translate(-center.to_vec2());

        self.transform_selection(affine);
        self.last_transform = Some(affine);
    }

    /// Skew selected points by (sx, sy) degrees around the
    /// selection center
    #[allow(dead_code)]
    pub fn skew_selection(&mut self, sx_deg: f64, sy_deg: f64) {
        let center = match self.selection_bounding_box() {
            Some(rect) => rect.center(),
            None => return,
        };

        let sx = (sx_deg * std::f64::consts::PI / 180.0).tan();
        let sy = (sy_deg * std::f64::consts::PI / 180.0).tan();

        // Translate to origin, skew, translate back
        let to_origin = kurbo::Affine::translate(-center.to_vec2());
        let skew = kurbo::Affine::skew(sx, sy);
        let back = kurbo::Affine::translate(center.to_vec2());
        let affine = back * skew * to_origin;

        self.transform_selection(affine);
        self.last_transform = Some(affine);
    }

    /// Duplicate selected contours
    ///
    /// Clones all paths containing selected points, assigns fresh
    /// EntityIds, offsets by (+20, +20), and updates the selection
    /// to the new points.
    pub fn duplicate_selection(&mut self) {
        use crate::model::EntityId;
        use crate::path::{
            CubicPath, HyperPath, PathPoint, PathPoints,
            QuadraticPath,
        };

        if self.selection.is_empty() {
            return;
        }

        let offset = kurbo::Vec2::new(20.0, 20.0);
        let selection = &self.selection;

        // Collect paths to duplicate
        let paths_to_dup: Vec<_> = self
            .paths
            .iter()
            .filter(|path| match path {
                Path::Cubic(c) => {
                    c.points.iter().any(|pt| selection.contains(&pt.id))
                }
                Path::Quadratic(q) => {
                    q.points.iter().any(|pt| selection.contains(&pt.id))
                }
                Path::Hyper(h) => {
                    h.points.iter().any(|pt| selection.contains(&pt.id))
                }
            })
            .cloned()
            .collect();

        if paths_to_dup.is_empty() {
            return;
        }

        let mut new_paths = Vec::new();
        let mut new_selection = Selection::new();

        for path in &paths_to_dup {
            match path {
                Path::Cubic(cubic) => {
                    let new_points: Vec<PathPoint> = cubic
                        .points
                        .iter()
                        .map(|pt| {
                            let id = EntityId::next();
                            new_selection.insert(id);
                            PathPoint {
                                id,
                                point: pt.point + offset,
                                typ: pt.typ,
                            }
                        })
                        .collect();
                    new_paths.push(Path::Cubic(CubicPath::new(
                        PathPoints::from_vec(new_points),
                        cubic.closed,
                    )));
                }
                Path::Quadratic(quad) => {
                    let new_points: Vec<PathPoint> = quad
                        .points
                        .iter()
                        .map(|pt| {
                            let id = EntityId::next();
                            new_selection.insert(id);
                            PathPoint {
                                id,
                                point: pt.point + offset,
                                typ: pt.typ,
                            }
                        })
                        .collect();
                    new_paths.push(Path::Quadratic(
                        QuadraticPath::new(
                            PathPoints::from_vec(new_points),
                            quad.closed,
                        ),
                    ));
                }
                Path::Hyper(hyper) => {
                    let new_points: Vec<PathPoint> = hyper
                        .points
                        .iter()
                        .map(|pt| {
                            let id = EntityId::next();
                            new_selection.insert(id);
                            PathPoint {
                                id,
                                point: pt.point + offset,
                                typ: pt.typ,
                            }
                        })
                        .collect();
                    let mut new_hyper = HyperPath::from_points(
                        PathPoints::from_vec(new_points),
                        hyper.closed,
                    );
                    new_hyper.after_change();
                    new_paths.push(Path::Hyper(new_hyper));
                }
            }
        }

        // Append new paths and update selection
        let paths_vec = Arc::make_mut(&mut self.paths);
        paths_vec.extend(new_paths);
        self.selection = new_selection;
    }

    // ================================================================
    // DELETE / TOGGLE / REVERSE
    // ================================================================

    /// Delete selected points
    ///
    /// This removes selected points from paths. If all points in a
    /// path are deleted, the entire path is removed.
    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }

        // Get mutable access to paths
        let paths_vec = Arc::make_mut(&mut self.paths);

        // Filter out paths that become empty after deletion
        paths_vec.retain_mut(|path| Self::retain_path_after_deletion(path, &self.selection));

        // Clear selection since deleted points are gone
        self.selection = Selection::new();
    }

    /// Toggle point type between smooth and corner for selected
    /// on-curve points
    pub fn toggle_point_type(&mut self) {
        if self.selection.is_empty() {
            return;
        }

        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            Self::toggle_points_in_path(path, &self.selection);
        }
    }

    /// Reverse the direction of all paths
    pub fn reverse_contours(&mut self) {
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            match path {
                Path::Cubic(cubic) => {
                    let points = cubic.points.make_mut();
                    points.reverse();
                }
                Path::Quadratic(quadratic) => {
                    let points = quadratic.points.make_mut();
                    points.reverse();
                }
                Path::Hyper(hyper) => {
                    let points = hyper.points.make_mut();
                    points.reverse();
                    hyper.after_change();
                }
            }
        }
    }

    /// Insert a point on a segment at position t
    ///
    /// This adds a new on-curve point to the path containing the
    /// given segment, at the parametric position t along that
    /// segment.
    ///
    /// For line segments: inserts one on-curve point
    /// For cubic curves: subdivides the curve, inserting 1 on-curve
    /// and 2 off-curve points
    ///
    /// Returns true if the point was successfully inserted.
    pub fn insert_point_on_segment(
        &mut self,
        segment_info: &crate::path::SegmentInfo,
        t: f64,
    ) -> bool {
        use crate::path::Segment;

        let paths_vec = Arc::make_mut(&mut self.paths);

        // Use the path_index stored in SegmentInfo to go directly
        // to the correct contour instead of searching by index
        // pairs (which are ambiguous across contours).
        let path = match paths_vec.get_mut(segment_info.path_index) {
            Some(p) => p,
            None => return false,
        };

        let points = match Self::get_path_points_mut(path) {
            Some(p) => p,
            None => return false,
        };

        match segment_info.segment {
            Segment::Line(_line) => {
                Self::insert_point_on_line(points, segment_info, t)
            }
            Segment::Cubic(cubic_bez) => {
                Self::insert_point_on_cubic(
                    points, segment_info, cubic_bez, t,
                )
            }
            Segment::Quadratic(quad_bez) => {
                Self::insert_point_on_quadratic(
                    points, segment_info, quad_bez, t,
                )
            }
        }
    }

    /// Convert the current editing state back to a Glyph
    ///
    /// This creates a new Glyph with the edited paths converted back
    /// to contours, preserving all other metadata from the original
    /// glyph.
    pub fn to_glyph(&self) -> Glyph {
        // Convert paths back to contours
        let contours: Vec<crate::model::workspace::Contour> =
            self.paths.iter().map(|path| path.to_contour()).collect();

        // Create updated glyph with new contours but preserve other
        // metadata (including components)
        Glyph {
            name: self.glyph.name.clone(),
            width: self.glyph.width,
            height: self.glyph.height,
            codepoints: self.glyph.codepoints.clone(),
            contours,
            components: self.glyph.components.clone(),
            left_group: self.glyph.left_group.clone(),
            right_group: self.glyph.right_group.clone(),
            mark_color: self.glyph.mark_color.clone(),
        }
    }

    /// Return the first selected on-curve point's entity ID,
    /// if any.
    pub fn first_selected_on_curve(
        &self,
    ) -> Option<crate::model::EntityId> {
        for path in self.paths.iter() {
            let points = match path {
                Path::Cubic(c) => &c.points,
                Path::Quadratic(q) => &q.points,
                Path::Hyper(h) => &h.points,
            };
            for pt in points.iter() {
                if pt.is_on_curve()
                    && self.selection.contains(&pt.id)
                {
                    return Some(pt.id);
                }
            }
        }
        None
    }

    /// Check whether an entity ID refers to an on-curve point.
    pub fn is_on_curve_point(
        &self,
        entity: crate::model::EntityId,
    ) -> bool {
        for path in self.paths.iter() {
            let points = match path {
                Path::Cubic(c) => &c.points,
                Path::Quadratic(q) => &q.points,
                Path::Hyper(h) => &h.points,
            };
            for pt in points.iter() {
                if pt.id == entity {
                    return pt.is_on_curve();
                }
            }
        }
        false
    }

    /// Set a specific on-curve point as the start node of its
    /// contour by rotating the point list.
    pub fn set_start_point(
        &mut self,
        entity: crate::model::EntityId,
    ) {
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            let (points, closed) = match path {
                Path::Cubic(c) => (&mut c.points, c.closed),
                Path::Quadratic(q) => {
                    (&mut q.points, q.closed)
                }
                Path::Hyper(h) => (&mut h.points, h.closed),
            };

            if !closed {
                continue;
            }

            // Find the point index
            let idx = points
                .iter()
                .position(|p| p.id == entity);
            let idx = match idx {
                Some(i) => i,
                None => continue,
            };

            if idx == 0 {
                return; // Already the start
            }

            // Rotate so this point is at index 0
            let mut vec = points.to_vec();
            vec.rotate_left(idx);
            *points =
                crate::path::point_list::PathPoints::from_vec(
                    vec,
                );
            return;
        }
    }

    /// Reverse the contour that contains the given point.
    pub fn reverse_contour_containing(
        &mut self,
        entity: crate::model::EntityId,
    ) {
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            let points = match path {
                Path::Cubic(c) => &mut c.points,
                Path::Quadratic(q) => &mut q.points,
                Path::Hyper(h) => &mut h.points,
            };

            let contains = points
                .iter()
                .any(|p| p.id == entity);
            if !contains {
                continue;
            }

            let mut vec = points.to_vec();
            vec.reverse();
            *points =
                crate::path::point_list::PathPoints::from_vec(
                    vec,
                );
            return;
        }
    }

    /// Find which contour (path index) contains the given entity.
    /// Returns None if the entity is not found in any contour.
    pub fn contour_index_for_entity(
        &self,
        entity: crate::model::EntityId,
    ) -> Option<usize> {
        for (i, path) in self.paths.iter().enumerate() {
            let points = match path {
                Path::Cubic(c) => &c.points,
                Path::Quadratic(q) => &q.points,
                Path::Hyper(h) => &h.points,
            };
            if points.iter().any(|p| p.id == entity) {
                return Some(i);
            }
        }
        None
    }

    /// Move a contour earlier in the contour list (toward
    /// index 0). This changes the contour order which
    /// affects interpolation compatibility.
    pub fn move_contour_up(
        &mut self,
        contour_index: usize,
    ) {
        if contour_index == 0
            || contour_index >= self.paths.len()
        {
            return;
        }
        let paths = Arc::make_mut(&mut self.paths);
        paths.swap(contour_index, contour_index - 1);
        self.selection = Selection::new();
    }

    /// Move a contour later in the contour list (toward the
    /// end). This changes the contour order which affects
    /// interpolation compatibility.
    pub fn move_contour_down(
        &mut self,
        contour_index: usize,
    ) {
        if contour_index + 1 >= self.paths.len() {
            return;
        }
        let paths = Arc::make_mut(&mut self.paths);
        paths.swap(contour_index, contour_index + 1);
        self.selection = Selection::new();
    }

    /// Apply a boolean operation (union, subtract, intersect, XOR)
    /// to all contours in the glyph.
    ///
    /// For Union: merges all overlapping contours into one.
    /// For Subtract/Intersect/Exclude: applies the operation
    /// between the first contour and all remaining contours.
    pub fn boolean_op(
        &mut self,
        op: linesweeper::BinaryOp,
    ) {
        use crate::editing::tracing::bezpath_to_cubic;

        if self.paths.len() < 2 {
            tracing::warn!(
                "Boolean ops need at least 2 contours"
            );
            return;
        }

        // Convert all paths to kurbo::BezPath
        let bezpaths: Vec<kurbo::BezPath> = self
            .paths
            .iter()
            .map(|p| p.to_bezpath())
            .collect();

        // Combine all contours into two BezPaths for the
        // binary op. For union, we put everything into set_a
        // and use an empty set_b. For other ops, first contour
        // is set_a, rest are set_b.
        let (set_a, set_b) = match op {
            linesweeper::BinaryOp::Union => {
                // Merge all contours into one BezPath
                let mut combined = kurbo::BezPath::new();
                for bp in &bezpaths {
                    for el in bp.elements() {
                        combined.push(*el);
                    }
                }
                (combined, kurbo::BezPath::new())
            }
            _ => {
                // First contour vs all others
                let mut rest = kurbo::BezPath::new();
                for bp in bezpaths.iter().skip(1) {
                    for el in bp.elements() {
                        rest.push(*el);
                    }
                }
                (bezpaths[0].clone(), rest)
            }
        };

        let result = match linesweeper::binary_op(
            &set_a,
            &set_b,
            linesweeper::FillRule::NonZero,
            op,
        ) {
            Ok(contours) => contours,
            Err(e) => {
                tracing::error!(
                    "Boolean operation failed: {e}"
                );
                return;
            }
        };

        // Convert result contours back to our Path type
        let new_paths: Vec<Path> = result
            .contours()
            .map(|contour| {
                Path::Cubic(bezpath_to_cubic(&contour.path))
            })
            .collect();

        if new_paths.is_empty() {
            tracing::warn!(
                "Boolean op produced no contours"
            );
            return;
        }

        tracing::info!(
            "Boolean op: {} contours → {} contours",
            self.paths.len(),
            new_paths.len()
        );

        self.selection = Selection::new();
        self.paths = Arc::new(new_paths);
    }

    /// Sync current edits to the workspace immediately
    ///
    /// This updates the workspace with the current editing state so that
    /// all instances of the glyph in the text buffer show the latest edits.
    /// Should be called after any edit operation (move, delete, add points, etc.)
    pub fn sync_to_workspace(&mut self) {
        // Only sync if we have an active sort and workspace
        let glyph_name = match &self.active_sort_name {
            Some(name) => name.clone(),
            None => return,
        };

        let workspace_lock = match &self.workspace {
            Some(ws) => ws,
            None => return,
        };

        // Get the updated glyph
        let updated_glyph = self.to_glyph();

        // Update both the session's glyph and the workspace
        self.glyph = Arc::new(updated_glyph.clone());

        // Update the workspace
        let mut workspace = write_workspace(workspace_lock);
        workspace.glyphs.insert(glyph_name, updated_glyph);
    }

    // ===== PRIVATE HELPERS =====

    /// Collect adjacent off-curve points for selected on-curve points
    fn collect_adjacent_off_curve_points(
        paths: &[Path],
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<crate::model::EntityId>,
    ) {
        for path in paths.iter() {
            match path {
                Path::Cubic(cubic) => {
                    Self::collect_adjacent_for_cubic(cubic, selection, points_to_move);
                }
                Path::Quadratic(quadratic) => {
                    Self::collect_adjacent_for_quadratic(quadratic, selection, points_to_move);
                }
                Path::Hyper(hyper) => {
                    Self::collect_adjacent_for_hyper(hyper, selection, points_to_move);
                }
            }
        }
    }

    /// Collect adjacent off-curve points for a cubic path
    fn collect_adjacent_for_cubic(
        cubic: &crate::path::CubicPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<crate::model::EntityId>,
    ) {
        let points: Vec<_> = cubic.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) = Self::get_previous_index(i, len, cubic.closed)
                    && prev_i < len
                    && points[prev_i].is_off_curve()
                {
                    points_to_move.insert(points[prev_i].id);
                }

                // Check next point
                if let Some(next_i) = Self::get_next_index(i, len, cubic.closed)
                    && next_i < len
                    && points[next_i].is_off_curve()
                {
                    points_to_move.insert(points[next_i].id);
                }
            }
        }
    }

    /// Collect adjacent off-curve points for a quadratic path
    fn collect_adjacent_for_quadratic(
        quadratic: &crate::path::QuadraticPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<crate::model::EntityId>,
    ) {
        let points: Vec<_> = quadratic.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) = Self::get_previous_index(i, len, quadratic.closed)
                    && prev_i < len
                    && points[prev_i].is_off_curve()
                {
                    points_to_move.insert(points[prev_i].id);
                }

                // Check next point
                if let Some(next_i) = Self::get_next_index(i, len, quadratic.closed)
                    && next_i < len
                    && points[next_i].is_off_curve()
                {
                    points_to_move.insert(points[next_i].id);
                }
            }
        }
    }

    /// Collect adjacent off-curve points for a hyper path
    fn collect_adjacent_for_hyper(
        hyper: &HyperPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<crate::model::EntityId>,
    ) {
        let points: Vec<_> = hyper.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) = Self::get_previous_index(i, len, hyper.closed)
                    && prev_i < len
                    && points[prev_i].is_off_curve()
                {
                    points_to_move.insert(points[prev_i].id);
                }

                // Check next point
                if let Some(next_i) = Self::get_next_index(i, len, hyper.closed)
                    && next_i < len
                    && points[next_i].is_off_curve()
                {
                    points_to_move.insert(points[next_i].id);
                }
            }
        }
    }

    /// Get the previous index in a path (with wrapping for closed
    /// paths)
    fn get_previous_index(current: usize, len: usize, closed: bool) -> Option<usize> {
        if current > 0 {
            Some(current - 1)
        } else if closed {
            Some(len - 1)
        } else {
            None
        }
    }

    /// Get the next index in a path (with wrapping for closed paths)
    fn get_next_index(current: usize, len: usize, closed: bool) -> Option<usize> {
        if current + 1 < len {
            Some(current + 1)
        } else if closed {
            Some(0)
        } else {
            None
        }
    }

    /// Apply point movement to paths
    fn apply_point_movement(
        paths: &mut [Path],
        points_to_move: &std::collections::HashSet<crate::model::EntityId>,
        delta: kurbo::Vec2,
    ) {
        for path in paths.iter_mut() {
            match path {
                Path::Cubic(cubic) => {
                    let points = cubic.points.make_mut();
                    Self::move_points_in_list(points, points_to_move, delta);
                }
                Path::Quadratic(quadratic) => {
                    let points = quadratic.points.make_mut();
                    Self::move_points_in_list(points, points_to_move, delta);
                }
                Path::Hyper(hyper) => {
                    let points = hyper.points.make_mut();
                    Self::move_points_in_list(points, points_to_move, delta);
                    hyper.after_change();
                }
            }
        }
    }

    /// Move points in a point list by delta
    fn move_points_in_list(
        points: &mut [crate::path::PathPoint],
        points_to_move: &std::collections::HashSet<crate::model::EntityId>,
        delta: kurbo::Vec2,
    ) {
        for point in points.iter_mut() {
            if points_to_move.contains(&point.id) {
                point.point = Point::new(point.point.x + delta.x, point.point.y + delta.y);
            }
        }
    }

    /// Enforce smooth constraints after handle movement
    ///
    /// For each smooth on-curve point: if exactly one adjacent
    /// off-curve handle was moved (and the on-curve itself was NOT
    /// moved), rotate the opposite handle to maintain collinearity,
    /// preserving its distance from the on-curve point.
    fn enforce_smooth_constraints(
        paths: &mut [Path],
        points_moved: &std::collections::HashSet<crate::model::EntityId>,
    ) {
        for path in paths.iter_mut() {
            match path {
                Path::Cubic(cubic) => {
                    Self::enforce_smooth_for_points(
                        cubic.points.make_mut(),
                        cubic.closed,
                        points_moved,
                    );
                }
                Path::Quadratic(quadratic) => {
                    Self::enforce_smooth_for_points(
                        quadratic.points.make_mut(),
                        quadratic.closed,
                        points_moved,
                    );
                }
                // Hyper paths have no user-visible off-curve handles
                Path::Hyper(_) => {}
            }
        }
    }

    /// Enforce smooth constraints on a single point list
    ///
    /// Iterates until convergence: adjusting one handle may break
    /// continuity at a neighboring smooth point, so we repeat until
    /// no more corrections are needed (or a safety cap is reached).
    ///
    /// Handles two cases per smooth on-curve point:
    /// 1. Two off-curve handles: both must be collinear through the
    ///    on-curve point with equal distance (G2).
    /// 2. One off-curve + one on-curve neighbor (line-to-curve
    ///    junction): the off-curve handle must lie on the ray from
    ///    the smooth point away from the on-curve neighbor, so the
    ///    tangent matches the line direction.
    fn enforce_smooth_for_points(
        points: &mut [crate::path::PathPoint],
        closed: bool,
        points_moved: &std::collections::HashSet<crate::model::EntityId>,
    ) {
        use std::collections::HashSet;

        let len = points.len();
        if len < 3 {
            return;
        }

        // Track which points have been disturbed. Start with the
        // originally moved set; each pass adds newly adjusted points.
        let mut disturbed: HashSet<crate::model::EntityId> =
            points_moved.clone();

        // Safety cap to avoid infinite loops
        const MAX_ITERATIONS: usize = 10;

        for _iteration in 0..MAX_ITERATIONS {
            let adjustments =
                Self::compute_smooth_adjustments(
                    points, closed, &disturbed,
                );

            if adjustments.is_empty() {
                break;
            }

            // Apply adjustments and track which points changed
            let mut changed_ids: HashSet<crate::model::EntityId> =
                HashSet::new();
            for (idx, pos) in &adjustments {
                let old = points[*idx].point;
                let dx = pos.x - old.x;
                let dy = pos.y - old.y;
                if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
                    points[*idx].point = *pos;
                    changed_ids.insert(points[*idx].id);
                }
            }

            if changed_ids.is_empty() {
                break;
            }

            // Next pass only looks at points affected by this pass
            disturbed = changed_ids;
        }
    }

    /// Single pass: compute smooth constraint adjustments
    fn compute_smooth_adjustments(
        points: &[crate::path::PathPoint],
        closed: bool,
        disturbed: &std::collections::HashSet<crate::model::EntityId>,
    ) -> Vec<(usize, Point)> {
        let len = points.len();
        let mut adjustments: Vec<(usize, Point)> = Vec::new();

        for i in 0..len {
            let is_smooth = matches!(
                points[i].typ,
                crate::path::PointType::OnCurve { smooth: true }
            );
            if !is_smooth {
                continue;
            }

            let prev_i = match Self::get_previous_index(
                i, len, closed,
            ) {
                Some(idx) => idx,
                None => continue,
            };
            let next_i =
                match Self::get_next_index(i, len, closed) {
                    Some(idx) => idx,
                    None => continue,
                };

            let prev_off = points[prev_i].is_off_curve();
            let next_off = points[next_i].is_off_curve();
            let oncurve = points[i].point;
            let oncurve_disturbed =
                disturbed.contains(&points[i].id);

            if prev_off && next_off {
                // Case 1: both neighbors are off-curve handles
                let prev_disturbed =
                    disturbed.contains(&points[prev_i].id);
                let next_disturbed =
                    disturbed.contains(&points[next_i].id);

                if oncurve_disturbed {
                    // On-curve moved — use prev as reference,
                    // constrain next
                    let new_pos = Self::constrained_opposite(
                        oncurve,
                        points[prev_i].point,
                        points[next_i].point,
                    );
                    let dx = new_pos.x - points[next_i].point.x;
                    let dy = new_pos.y - points[next_i].point.y;
                    if dx.abs() > 1e-6 || dy.abs() > 1e-6 {
                        adjustments.push((next_i, new_pos));
                    }
                } else if prev_disturbed && !next_disturbed {
                    let new_pos = Self::constrained_opposite(
                        oncurve,
                        points[prev_i].point,
                        points[next_i].point,
                    );
                    adjustments.push((next_i, new_pos));
                } else if next_disturbed && !prev_disturbed {
                    let new_pos = Self::constrained_opposite(
                        oncurve,
                        points[next_i].point,
                        points[prev_i].point,
                    );
                    adjustments.push((prev_i, new_pos));
                }
            } else if prev_off && !next_off {
                // Case 2a: handle on prev side, line on next side
                // Trigger if on-curve, handle, OR line neighbor moved
                if !oncurve_disturbed
                    && !disturbed.contains(&points[prev_i].id)
                    && !disturbed.contains(&points[next_i].id)
                {
                    continue;
                }
                let new_pos = Self::constrain_handle_to_line(
                    oncurve,
                    points[next_i].point,
                    points[prev_i].point,
                );
                adjustments.push((prev_i, new_pos));
            } else if !prev_off && next_off {
                // Case 2b: line on prev side, handle on next side
                // Trigger if on-curve, handle, OR line neighbor moved
                if !oncurve_disturbed
                    && !disturbed.contains(&points[next_i].id)
                    && !disturbed.contains(&points[prev_i].id)
                {
                    continue;
                }
                let new_pos = Self::constrain_handle_to_line(
                    oncurve,
                    points[prev_i].point,
                    points[next_i].point,
                );
                adjustments.push((next_i, new_pos));
            }
        }

        adjustments
    }

    /// Constrain a handle at a line-to-curve junction
    ///
    /// At a smooth on-curve point where one neighbor is on-curve
    /// (forming a line segment) and the other is off-curve (a curve
    /// handle), the handle must lie on the ray from the on-curve
    /// point away from the line neighbor. This ensures the curve
    /// leaves the on-curve point tangent to the line segment (G1).
    ///
    /// The handle's distance from the on-curve point is preserved;
    /// only its direction is constrained.
    fn constrain_handle_to_line(
        oncurve: Point,
        line_neighbor: Point,
        handle: Point,
    ) -> Point {
        // Direction from line neighbor through on-curve point
        let ray_dx = oncurve.x - line_neighbor.x;
        let ray_dy = oncurve.y - line_neighbor.y;
        let ray_len = (ray_dx * ray_dx + ray_dy * ray_dy).sqrt();
        if ray_len < 1e-9 {
            return handle; // Degenerate: line neighbor is on top
        }

        // Unit direction along the ray (away from line neighbor)
        let ux = ray_dx / ray_len;
        let uy = ray_dy / ray_len;

        // Preserve handle's distance from on-curve
        let hx = handle.x - oncurve.x;
        let hy = handle.y - oncurve.y;
        let handle_dist = (hx * hx + hy * hy).sqrt();
        if handle_dist < 1e-9 {
            return handle; // Handle is on top of on-curve
        }

        // Project handle onto the ray direction (must be positive =
        // away from line neighbor)
        Point::new(
            oncurve.x + ux * handle_dist,
            oncurve.y + uy * handle_dist,
        )
    }

    /// Compute the constrained position of the opposite handle
    ///
    /// Enforces G1 continuity: the opposite handle is rotated to be
    /// collinear with the moved handle through the on-curve point,
    /// but its original distance from the on-curve point is
    /// preserved. This allows handles to have different lengths
    /// (asymmetric curvature) while maintaining a smooth transition.
    fn constrained_opposite(
        oncurve: Point,
        moved_handle: Point,
        opposite: Point,
    ) -> Point {
        let dx = moved_handle.x - oncurve.x;
        let dy = moved_handle.y - oncurve.y;
        let moved_dist = (dx * dx + dy * dy).sqrt();
        if moved_dist < 1e-9 {
            return opposite; // Degenerate
        }

        // Preserve the opposite handle's original distance
        let ox = opposite.x - oncurve.x;
        let oy = opposite.y - oncurve.y;
        let opp_dist = (ox * ox + oy * oy).sqrt();
        if opp_dist < 1e-9 {
            return opposite; // Degenerate
        }

        // Direction from moved handle through on-curve (opposite)
        let angle = dy.atan2(dx);
        let opposite_angle = angle + std::f64::consts::PI;
        Point::new(
            oncurve.x + opp_dist * opposite_angle.cos(),
            oncurve.y + opp_dist * opposite_angle.sin(),
        )
    }

    /// Reverse contours that contain any of the given point IDs
    ///
    /// Used after reflection transforms (negative determinant) to
    /// maintain correct winding direction.
    fn reverse_affected_contours(
        paths: &mut [Path],
        affected: &std::collections::HashSet<crate::model::EntityId>,
    ) {
        for path in paths.iter_mut() {
            let contains_affected = match path {
                Path::Cubic(c) => {
                    c.points.iter().any(|pt| affected.contains(&pt.id))
                }
                Path::Quadratic(q) => {
                    q.points.iter().any(|pt| affected.contains(&pt.id))
                }
                Path::Hyper(h) => {
                    h.points.iter().any(|pt| affected.contains(&pt.id))
                }
            };

            if !contains_affected {
                continue;
            }

            match path {
                Path::Cubic(c) => c.points.make_mut().reverse(),
                Path::Quadratic(q) => {
                    q.points.make_mut().reverse()
                }
                Path::Hyper(h) => {
                    h.points.make_mut().reverse();
                    h.after_change();
                }
            }
        }
    }

    /// Retain a path after deletion (remove selected points)
    fn retain_path_after_deletion(path: &mut Path, selection: &Selection) -> bool {
        match path {
            Path::Cubic(cubic) => {
                let points = cubic.points.make_mut();
                points.retain(|point| !selection.contains(&point.id));
                points.len() >= 2
            }
            Path::Quadratic(quadratic) => {
                let points = quadratic.points.make_mut();
                points.retain(|point| !selection.contains(&point.id));
                points.len() >= 2
            }
            Path::Hyper(hyper) => {
                let points = hyper.points.make_mut();
                points.retain(|point| !selection.contains(&point.id));
                let len = points.len();
                hyper.after_change();
                len >= 2
            }
        }
    }

    /// Toggle point types in a path
    fn toggle_points_in_path(path: &mut Path, selection: &Selection) {
        match path {
            Path::Cubic(cubic) => {
                let points = cubic.points.make_mut();
                Self::toggle_points_in_list(points, selection);
            }
            Path::Quadratic(quadratic) => {
                let points = quadratic.points.make_mut();
                Self::toggle_points_in_list(points, selection);
            }
            Path::Hyper(hyper) => {
                let points = hyper.points.make_mut();
                Self::toggle_points_in_list(points, selection);
                hyper.after_change();
            }
        }
    }

    /// Toggle point types in a point list
    fn toggle_points_in_list(points: &mut [crate::path::PathPoint], selection: &Selection) {
        for point in points.iter_mut() {
            if selection.contains(&point.id) {
                // Only toggle on-curve points
                if let crate::path::PointType::OnCurve { smooth } = &mut point.typ {
                    *smooth = !*smooth;
                }
            }
        }
    }

    /// Get mutable access to a path's point list
    fn get_path_points_mut(
        path: &mut Path,
    ) -> Option<&mut Vec<crate::path::PathPoint>> {
        match path {
            Path::Cubic(cubic) => Some(cubic.points.make_mut()),
            Path::Quadratic(quadratic) => {
                Some(quadratic.points.make_mut())
            }
            Path::Hyper(hyper) => Some(hyper.points.make_mut()),
        }
    }

    /// Insert a point on a line segment
    fn insert_point_on_line(
        points: &mut Vec<crate::path::PathPoint>,
        segment_info: &crate::path::SegmentInfo,
        t: f64,
    ) -> bool {
        use crate::model::EntityId;
        use crate::path::{PathPoint, PointType};

        let point_pos = segment_info.segment.eval(t);
        let point_pos = snap_point_to_grid(point_pos);
        let new_point = PathPoint {
            id: EntityId::next(),
            point: point_pos,
            typ: PointType::OnCurve { smooth: false },
        };

        // Insert between start and end
        let insert_idx = segment_info.end_index;
        points.insert(insert_idx, new_point);

        true
    }

    /// Insert a point on a cubic curve segment
    fn insert_point_on_cubic(
        points: &mut Vec<crate::path::PathPoint>,
        segment_info: &crate::path::SegmentInfo,
        cubic_bez: kurbo::CubicBez,
        t: f64,
    ) -> bool {
        use crate::path::Segment;

        // For a cubic curve, subdivide it using de Casteljau
        // algorithm
        let (left, right) = Segment::subdivide_cubic(cubic_bez, t);

        // Create the new points from subdivision
        let new_points = Self::create_cubic_subdivision_points(left, right);

        // Calculate how many points are between start and end
        let points_between = Self::calculate_points_between(
            segment_info.start_index,
            segment_info.end_index,
            points.len(),
        );

        // Remove the old control points
        if points_between > 0 {
            for _ in 0..points_between {
                points.remove(segment_info.start_index + 1);
            }
        }

        // Insert the new points after start_index
        let mut insert_idx = segment_info.start_index + 1;
        for new_point in new_points {
            points.insert(insert_idx, new_point);
            insert_idx += 1;
        }

        true
    }

    /// Create points from cubic curve subdivision
    fn create_cubic_subdivision_points(
        left: kurbo::CubicBez,
        right: kurbo::CubicBez,
    ) -> Vec<crate::path::PathPoint> {
        use crate::model::EntityId;
        use crate::path::{PathPoint, PointType};

        vec![
            PathPoint {
                id: EntityId::next(),
                point: left.p1,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: left.p2,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: left.p3, // Same as right.p0
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: right.p1,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: right.p2,
                typ: PointType::OffCurve { auto: false },
            },
        ]
    }

    /// Insert a point on a quadratic curve segment
    fn insert_point_on_quadratic(
        points: &mut Vec<crate::path::PathPoint>,
        segment_info: &crate::path::SegmentInfo,
        quad_bez: kurbo::QuadBez,
        t: f64,
    ) -> bool {
        use crate::model::EntityId;
        use crate::path::Segment;
        use crate::path::{PathPoint, PointType};

        // For a quadratic curve, subdivide it using de Casteljau
        // algorithm
        let (left, right) = Segment::subdivide_quadratic(quad_bez, t);

        // Create the new points from subdivision
        let new_points = vec![
            PathPoint {
                id: EntityId::next(),
                point: left.p1,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: left.p2, // Same as right.p0
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: right.p1,
                typ: PointType::OffCurve { auto: false },
            },
        ];

        // Calculate how many points are between start and end
        let points_between = Self::calculate_points_between(
            segment_info.start_index,
            segment_info.end_index,
            points.len(),
        );

        // Remove the old control point
        if points_between > 0 {
            points.remove(segment_info.start_index + 1);
        }

        // Insert the new points after start_index
        let mut insert_idx = segment_info.start_index + 1;
        for new_point in new_points {
            points.insert(insert_idx, new_point);
            insert_idx += 1;
        }

        true
    }

    /// Calculate how many points are between start and end indices
    fn calculate_points_between(start_index: usize, end_index: usize, total_len: usize) -> usize {
        if end_index > start_index {
            end_index - start_index - 1
        } else {
            // Handle wrap-around for closed paths
            total_len - start_index - 1 + end_index
        }
    }
}
