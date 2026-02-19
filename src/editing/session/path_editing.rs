// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Path editing methods for EditSession — point manipulation,
//! deletion, contour operations, and glyph conversion

use super::EditSession;
use crate::editing::selection::Selection;
use crate::model::workspace::Glyph;
use crate::path::{HyperPath, Path};
use kurbo::Point;
use std::sync::Arc;

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
    }

    /// Nudge selected points in a direction
    ///
    /// Nudge amounts:
    /// - Normal: 1 unit
    /// - Shift: 10 units
    /// - Cmd/Ctrl: 100 units
    /// TODO: move this to settings and make constants
    pub fn nudge_selection(&mut self, dx: f64, dy: f64, shift: bool, ctrl: bool) {
        let multiplier = if ctrl {
            100.0
        } else if shift {
            10.0
        } else {
            1.0
        };

        let delta = kurbo::Vec2::new(dx * multiplier, dy * multiplier);
        self.move_selection(delta);
    }

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

        // Find the path containing this segment
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            if let Some(points) = Self::find_path_containing_segment(path, segment_info) {
                match segment_info.segment {
                    Segment::Line(_line) => {
                        return Self::insert_point_on_line(points, segment_info, t);
                    }
                    Segment::Cubic(cubic_bez) => {
                        return Self::insert_point_on_cubic(points, segment_info, cubic_bez, t);
                    }
                    Segment::Quadratic(quad_bez) => {
                        return Self::insert_point_on_quadratic(points, segment_info, quad_bez, t);
                    }
                }
            }
        }

        false
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
        let mut workspace = workspace_lock.write().unwrap();
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

    /// Find the path containing a segment and return its points
    fn find_path_containing_segment<'a>(
        path: &'a mut Path,
        segment_info: &crate::path::SegmentInfo,
    ) -> Option<&'a mut Vec<crate::path::PathPoint>> {
        match path {
            Path::Cubic(cubic) => {
                if Self::cubic_contains_segment(cubic, segment_info) {
                    Some(cubic.points.make_mut())
                } else {
                    None
                }
            }
            Path::Quadratic(quadratic) => {
                if Self::quadratic_contains_segment(quadratic, segment_info) {
                    Some(quadratic.points.make_mut())
                } else {
                    None
                }
            }
            Path::Hyper(hyper) => {
                if Self::hyper_contains_segment(hyper, segment_info) {
                    Some(hyper.points.make_mut())
                } else {
                    None
                }
            }
        }
    }

    /// Check if a cubic path contains a specific segment
    fn cubic_contains_segment(
        cubic: &crate::path::CubicPath,
        segment_info: &crate::path::SegmentInfo,
    ) -> bool {
        for seg in cubic.iter_segments() {
            if seg.start_index == segment_info.start_index
                && seg.end_index == segment_info.end_index
            {
                return true;
            }
        }
        false
    }

    /// Check if a quadratic path contains a specific segment
    fn quadratic_contains_segment(
        quadratic: &crate::path::QuadraticPath,
        segment_info: &crate::path::SegmentInfo,
    ) -> bool {
        for seg in quadratic.iter_segments() {
            if seg.start_index == segment_info.start_index
                && seg.end_index == segment_info.end_index
            {
                return true;
            }
        }
        false
    }

    /// Check if a hyper path contains a specific segment
    fn hyper_contains_segment(hyper: &HyperPath, segment_info: &crate::path::SegmentInfo) -> bool {
        for seg in hyper.iter_segments() {
            if seg.start_index == segment_info.start_index
                && seg.end_index == segment_info.end_index
            {
                return true;
            }
        }
        false
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
