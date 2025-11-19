// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Edit session - manages editing state for a single glyph

use crate::components::CoordinateSelection;
use crate::hit_test::{self, HitTestResult};
use crate::hyper_path::HyperPath;
use crate::path::Path;
use crate::selection::Selection;
use crate::tools::{ToolBox, ToolId};
use crate::viewport::ViewPort;
use crate::workspace::Glyph;
use kurbo::{Point, Rect};
use std::sync::Arc;

// CoordinateSelection has been moved to components::coordinate_panel
// module

/// Editing session for a single glyph
///
/// This holds all the state needed to edit a glyph, including the
/// outline data, selection, viewport, and metadata.
#[derive(Debug, Clone)]
pub struct EditSession {

    /// Name of the glyph being edited
    pub glyph_name: String,

    /// Path to the UFO file
    pub ufo_path: std::path::PathBuf,

    /// The original glyph data (for metadata, unicode, etc.)
    pub glyph: Arc<Glyph>,

    /// The editable path representation (converted from glyph
    /// contours)
    pub paths: Arc<Vec<Path>>,

    /// Currently selected entities (points, paths, etc.)
    pub selection: Selection,

    /// Coordinate selection (for the coordinate pane)
    pub coord_selection: CoordinateSelection,

    /// Current editing tool
    pub current_tool: ToolBox,

    /// Viewport transformation
    pub viewport: ViewPort,

    /// Whether the viewport has been initialized (to avoid
    /// recalculating on every frame)
    pub viewport_initialized: bool,

    /// Font metrics (for drawing guides)
    #[allow(dead_code)] // Stored for potential future use
    pub units_per_em: f64,
    pub ascender: f64,
    pub descender: f64,
    pub x_height: Option<f64>,
    pub cap_height: Option<f64>,
}

impl EditSession {
    /// Create a new editing session for a glyph
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        glyph_name: String,
        ufo_path: std::path::PathBuf,
        glyph: Glyph,
        units_per_em: f64,
        ascender: f64,
        descender: f64,
        x_height: Option<f64>,
        cap_height: Option<f64>,
    ) -> Self {
        // Convert glyph contours to editable paths
        let paths: Vec<Path> = glyph
            .contours
            .iter()
            .map(Path::from_contour)
            .collect();

        Self {
            glyph_name,
            ufo_path,
            glyph: Arc::new(glyph),
            paths: Arc::new(paths),
            selection: Selection::new(),
            coord_selection: CoordinateSelection::default(),
            current_tool: ToolBox::for_id(ToolId::Select),
            viewport: ViewPort::new(),
            viewport_initialized: false,
            units_per_em,
            ascender,
            descender,
            x_height,
            cap_height,
        }
    }

    /// Compute the coordinate selection from the current selection
    ///
    /// This calculates the bounding box of all selected points and
    /// updates the coord_selection field.
    pub fn update_coord_selection(&mut self) {
        if self.selection.is_empty() {
            self.coord_selection = CoordinateSelection::default();
            return;
        }

        let bbox = Self::calculate_selection_bbox(
            &self.paths,
            &self.selection,
        );

        match bbox {
            Some((count, frame)) => {
                self.coord_selection = CoordinateSelection::new(
                    count,
                    frame,
                    // Preserve the current quadrant selection
                    self.coord_selection.quadrant,
                );
            }
            None => {
                self.coord_selection = CoordinateSelection::default();
            }
        }
    }


    /// Hit test for a point at screen coordinates
    ///
    /// Returns the EntityId of the closest point within max_dist
    /// screen pixels
    pub fn hit_test_point(
        &self,
        screen_pos: Point,
        max_dist: Option<f64>,
    ) -> Option<HitTestResult> {
        let max_dist = max_dist.unwrap_or(hit_test::MIN_CLICK_DISTANCE);

        // Collect all points from all paths as screen coordinates
        let candidates = self.paths.iter().flat_map(|path| {
            Self::path_to_hit_candidates(path, &self.viewport)
        });

        // Find closest point in screen space
        hit_test::find_closest(screen_pos, candidates, max_dist)
    }

    /// Hit test for path segments at screen coordinates
    ///
    /// Returns the closest segment within max_dist screen pixels,
    /// along with the parametric position (t) on that segment where
    /// the nearest point lies.
    ///
    /// The parameter t ranges from 0.0 (start of segment) to 1.0
    /// (end of segment).
    pub fn hit_test_segments(
        &self,
        screen_pos: Point,
        max_dist: f64,
    ) -> Option<(crate::path_segment::SegmentInfo, f64)> {
        // Convert screen position to design space
        let design_pos = self.viewport.screen_to_design(screen_pos);

        let closest_segment = Self::find_closest_segment(
            &self.paths,
            design_pos,
        );

        // Check if the closest segment is within max_dist
        closest_segment.and_then(|(segment_info, t, dist_sq)| {
            // Convert max_dist from screen pixels to design units
            let max_dist_design = max_dist / self.viewport.zoom;
            let max_dist_sq = max_dist_design * max_dist_design;

            if dist_sq <= max_dist_sq {
                Some((segment_info, t))
            } else {
                None
            }
        })
    }

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

        use crate::entity_id::EntityId;
        use std::collections::HashSet;

        // We need to mutate paths, so convert Arc<Vec<Path>> to
        // mutable Vec
        let paths_vec = Arc::make_mut(&mut self.paths);

        // Build a set of IDs to move:
        // - All selected points
        // - Adjacent off-curve points of selected on-curve points
        let mut points_to_move: HashSet<EntityId> =
            self.selection.iter().copied().collect();

        // First pass: identify adjacent off-curve points of selected
        // on-curve points
        Self::collect_adjacent_off_curve_points(
            paths_vec,
            &self.selection,
            &mut points_to_move,
        );

        // Second pass: move all identified points
        Self::apply_point_movement(paths_vec, &points_to_move, delta);
    }

    /// Nudge selected points in a direction
    ///
    /// Nudge amounts:
    /// - Normal: 1 unit
    /// - Shift: 10 units
    /// - Cmd/Ctrl: 100 units
    pub fn nudge_selection(
        &mut self,
        dx: f64,
        dy: f64,
        shift: bool,
        ctrl: bool,
    ) {
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
        paths_vec.retain_mut(|path| {
            Self::retain_path_after_deletion(path, &self.selection)
        });

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
        segment_info: &crate::path_segment::SegmentInfo,
        t: f64,
    ) -> bool {
        use crate::path_segment::Segment;

        // Find the path containing this segment
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            if let Some(points) =
                Self::find_path_containing_segment(path, segment_info)
            {
                match segment_info.segment {
                    Segment::Line(_line) => {
                        return Self::insert_point_on_line(
                            points,
                            segment_info,
                            t,
                        );
                    }
                    Segment::Cubic(cubic_bez) => {
                        return Self::insert_point_on_cubic(
                            points,
                            segment_info,
                            cubic_bez,
                            t,
                        );
                    }
                    Segment::Quadratic(quad_bez) => {
                        return Self::insert_point_on_quadratic(
                            points,
                            segment_info,
                            quad_bez,
                            t,
                        );
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
        use crate::workspace::Glyph;

        // Convert paths back to contours
        let contours: Vec<crate::workspace::Contour> =
            self.paths.iter().map(|path| path.to_contour()).collect();

        // Create updated glyph with new contours but preserve other
        // metadata
        Glyph {
            name: self.glyph.name.clone(),
            width: self.glyph.width,
            height: self.glyph.height,
            codepoints: self.glyph.codepoints.clone(),
            contours,
        }
    }

    // ===== HELPER METHODS =====

    /// Calculate the bounding box of selected points
    fn calculate_selection_bbox(
        paths: &[Path],
        selection: &Selection,
    ) -> Option<(usize, Rect)> {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut count = 0;

        for path in paths.iter() {
            Self::collect_selected_points_from_path(
                path,
                selection,
                &mut min_x,
                &mut max_x,
                &mut min_y,
                &mut max_y,
                &mut count,
            );
        }

        if min_x.is_finite() {
            let frame = Rect::new(min_x, min_y, max_x, max_y);
            Some((count, frame))
        } else {
            None
        }
    }

    /// Collect selected points from a path for bounding box
    /// calculation
    fn collect_selected_points_from_path(
        path: &Path,
        selection: &Selection,
        min_x: &mut f64,
        max_x: &mut f64,
        min_y: &mut f64,
        max_y: &mut f64,
        count: &mut usize,
    ) {
        let points_iter: Box<dyn Iterator<Item = _>> = match path {
            Path::Cubic(cubic) => {
                Box::new(cubic.points.iter())
            }
            Path::Quadratic(quadratic) => {
                Box::new(quadratic.points.iter())
            }
            Path::Hyper(hyper) => {
                Box::new(hyper.points.iter())
            }
        };

        for pt in points_iter {
            if selection.contains(&pt.id) {
                *min_x = (*min_x).min(pt.point.x);
                *max_x = (*max_x).max(pt.point.x);
                *min_y = (*min_y).min(pt.point.y);
                *max_y = (*max_y).max(pt.point.y);
                *count += 1;
            }
        }
    }

    /// Convert a path to hit test candidates (for point hit testing)
    fn path_to_hit_candidates(
        path: &Path,
        viewport: &ViewPort,
    ) -> Vec<(crate::entity_id::EntityId, Point, bool)> {
        match path {
            Path::Cubic(cubic) => cubic
                .points()
                .iter()
                .map(|pt| {
                    let screen_pt = viewport.to_screen(pt.point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
            Path::Quadratic(quadratic) => quadratic
                .points()
                .iter()
                .map(|pt| {
                    let screen_pt = viewport.to_screen(pt.point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
            Path::Hyper(hyper) => hyper
                .points()
                .iter()
                .map(|pt| {
                    let screen_pt = viewport.to_screen(pt.point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
        }
    }

    /// Find the closest segment to a design space point
    fn find_closest_segment(
        paths: &[Path],
        design_pos: kurbo::Point,
    ) -> Option<(
        crate::path_segment::SegmentInfo,
        f64,
        f64,
    )> {
        let mut closest: Option<(
            crate::path_segment::SegmentInfo,
            f64,
            f64,
        )> = None;

        for path in paths.iter() {
            Self::process_path_segments(path, design_pos, &mut closest);
        }
        closest
    }

    /// Process segments from a single path and update closest segment
    fn process_path_segments(
        path: &Path,
        design_pos: kurbo::Point,
        closest: &mut Option<(
            crate::path_segment::SegmentInfo,
            f64,
            f64,
        )>,
    ) {
        match path {
            Path::Cubic(cubic) => {
                Self::process_path_segment_iterator(
                    cubic.iter_segments(),
                    design_pos,
                    closest,
                );
            }
            Path::Quadratic(quadratic) => {
                Self::process_path_segment_iterator(
                    quadratic.iter_segments(),
                    design_pos,
                    closest,
                );
            }
            Path::Hyper(hyper) => {
                Self::process_path_segment_iterator(
                    hyper.iter_segments(),
                    design_pos,
                    closest,
                );
            }
        }
    }

    /// Process an iterator of segments and update closest segment
    fn process_path_segment_iterator<I>(
        segments: I,
        design_pos: kurbo::Point,
        closest: &mut Option<(
            crate::path_segment::SegmentInfo,
            f64,
            f64,
        )>,
    ) where
        I: Iterator<Item = crate::path_segment::SegmentInfo>,
    {
        for segment_info in segments {
            let (t, dist_sq) = segment_info.segment.nearest(design_pos);
            Self::update_closest_segment(
                closest,
                segment_info,
                t,
                dist_sq,
            );
        }
    }

    /// Update the closest segment if this one is closer
    fn update_closest_segment(
        closest: &mut Option<(crate::path_segment::SegmentInfo, f64, f64)>,
        segment_info: crate::path_segment::SegmentInfo,
        t: f64,
        dist_sq: f64,
    ) {
        match closest {
            None => {
                *closest = Some((segment_info, t, dist_sq));
            }
            Some((_, _, best_dist_sq)) => {
                if dist_sq < *best_dist_sq {
                    *closest = Some((segment_info, t, dist_sq));
                }
            }
        }
    }

    /// Collect adjacent off-curve points for selected on-curve points
    fn collect_adjacent_off_curve_points(
        paths: &[Path],
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        for path in paths.iter() {
            match path {
                Path::Cubic(cubic) => {
                    Self::collect_adjacent_for_cubic(
                        cubic,
                        selection,
                        points_to_move,
                    );
                }
                Path::Quadratic(quadratic) => {
                    Self::collect_adjacent_for_quadratic(
                        quadratic,
                        selection,
                        points_to_move,
                    );
                }
                Path::Hyper(hyper) => {
                    Self::collect_adjacent_for_hyper(
                        hyper,
                        selection,
                        points_to_move,
                    );
                }
            }
        }
    }

    /// Collect adjacent off-curve points for a cubic path
    fn collect_adjacent_for_cubic(
        cubic: &crate::cubic_path::CubicPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        let points: Vec<_> = cubic.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) =
                    Self::get_previous_index(i, len, cubic.closed)
                    && prev_i < len && points[prev_i].is_off_curve() {
                        points_to_move.insert(points[prev_i].id);
                    }

                // Check next point
                if let Some(next_i) = Self::get_next_index(i, len, cubic.closed)
                    && next_i < len && points[next_i].is_off_curve() {
                        points_to_move.insert(points[next_i].id);
                    }
            }
        }
    }

    /// Collect adjacent off-curve points for a quadratic path
    fn collect_adjacent_for_quadratic(
        quadratic: &crate::quadratic_path::QuadraticPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        let points: Vec<_> = quadratic.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) =
                    Self::get_previous_index(i, len, quadratic.closed)
                    && prev_i < len && points[prev_i].is_off_curve() {
                        points_to_move.insert(points[prev_i].id);
                    }

                // Check next point
                if let Some(next_i) =
                    Self::get_next_index(i, len, quadratic.closed)
                    && next_i < len && points[next_i].is_off_curve() {
                        points_to_move.insert(points[next_i].id);
                    }
            }
        }
    }

    /// Collect adjacent off-curve points for a hyper path
    fn collect_adjacent_for_hyper(
        hyper: &HyperPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        let points: Vec<_> = hyper.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) =
                    Self::get_previous_index(i, len, hyper.closed)
                    && prev_i < len && points[prev_i].is_off_curve() {
                        points_to_move.insert(points[prev_i].id);
                    }

                // Check next point
                if let Some(next_i) =
                    Self::get_next_index(i, len, hyper.closed)
                    && next_i < len && points[next_i].is_off_curve() {
                        points_to_move.insert(points[next_i].id);
                    }
            }
        }
    }

    /// Get the previous index in a path (with wrapping for closed
    /// paths)
    fn get_previous_index(
        current: usize,
        len: usize,
        closed: bool,
    ) -> Option<usize> {
        if current > 0 {
            Some(current - 1)
        } else if closed {
            Some(len - 1)
        } else {
            None
        }
    }

    /// Get the next index in a path (with wrapping for closed paths)
    fn get_next_index(
        current: usize,
        len: usize,
        closed: bool,
    ) -> Option<usize> {
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
        points_to_move: &std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
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
        points: &mut [crate::point::PathPoint],
        points_to_move: &std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
        delta: kurbo::Vec2,
    ) {
        for point in points.iter_mut() {
            if points_to_move.contains(&point.id) {
                point.point = Point::new(
                    point.point.x + delta.x,
                    point.point.y + delta.y,
                );
            }
        }
    }

    /// Retain a path after deletion (remove selected points)
    fn retain_path_after_deletion(
        path: &mut Path,
        selection: &Selection,
    ) -> bool {
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
    fn toggle_points_in_list(
        points: &mut [crate::point::PathPoint],
        selection: &Selection,
    ) {
        for point in points.iter_mut() {
            if selection.contains(&point.id) {
                // Only toggle on-curve points
                if let crate::point::PointType::OnCurve { smooth } =
                    &mut point.typ
                {
                    *smooth = !*smooth;
                }
            }
        }
    }

    /// Find the path containing a segment and return its points
    fn find_path_containing_segment<'a>(
        path: &'a mut Path,
        segment_info: &crate::path_segment::SegmentInfo,
    ) -> Option<&'a mut Vec<crate::point::PathPoint>> {
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
        cubic: &crate::cubic_path::CubicPath,
        segment_info: &crate::path_segment::SegmentInfo,
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
        quadratic: &crate::quadratic_path::QuadraticPath,
        segment_info: &crate::path_segment::SegmentInfo,
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
    fn hyper_contains_segment(
        hyper: &HyperPath,
        segment_info: &crate::path_segment::SegmentInfo,
    ) -> bool {
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
        points: &mut Vec<crate::point::PathPoint>,
        segment_info: &crate::path_segment::SegmentInfo,
        t: f64,
    ) -> bool {
        use crate::entity_id::EntityId;
        use crate::point::{PathPoint, PointType};

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
        points: &mut Vec<crate::point::PathPoint>,
        segment_info: &crate::path_segment::SegmentInfo,
        cubic_bez: kurbo::CubicBez,
        t: f64,
    ) -> bool {
        use crate::path_segment::Segment;

        // For a cubic curve, subdivide it using de Casteljau
        // algorithm
        let (left, right) = Segment::subdivide_cubic(cubic_bez, t);

        // Create the new points from subdivision
        let new_points = Self::create_cubic_subdivision_points(
            left,
            right,
        );

        // Calculate how many points are between start and end
        let points_between =
            Self::calculate_points_between(
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
    ) -> Vec<crate::point::PathPoint> {
        use crate::entity_id::EntityId;
        use crate::point::{PathPoint, PointType};

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
        points: &mut Vec<crate::point::PathPoint>,
        segment_info: &crate::path_segment::SegmentInfo,
        quad_bez: kurbo::QuadBez,
        t: f64,
    ) -> bool {
        use crate::entity_id::EntityId;
        use crate::point::{PathPoint, PointType};
        use crate::path_segment::Segment;

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
    fn calculate_points_between(
        start_index: usize,
        end_index: usize,
        total_len: usize,
    ) -> usize {
        if end_index > start_index {
            end_index - start_index - 1
        } else {
            // Handle wrap-around for closed paths
            total_len - start_index - 1 + end_index
        }
    }
}

