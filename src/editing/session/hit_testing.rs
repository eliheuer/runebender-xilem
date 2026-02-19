// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Hit testing and coordinate selection methods for EditSession

use super::EditSession;
use crate::components::CoordinateSelection;
use crate::editing::hit_test::{self, HitTestResult};
use crate::editing::selection::Selection;
use crate::editing::viewport::ViewPort;
use crate::path::Path;
use kurbo::{Point, Rect};

impl EditSession {
    /// Compute the coordinate selection from the current selection
    ///
    /// This calculates the bounding box of all selected points and
    /// updates the coord_selection field.
    pub fn update_coord_selection(&mut self) {
        if self.selection.is_empty() {
            self.coord_selection = CoordinateSelection::default();
            return;
        }

        let bbox = Self::calculate_selection_bbox(&self.paths, &self.selection);

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

        tracing::debug!(
            "[hit_test_point] screen_pos=({}, {}), offset={}, max_dist={}",
            screen_pos.x,
            screen_pos.y,
            self.active_sort_x_offset,
            max_dist
        );

        // Collect all points from all paths as screen coordinates
        // Apply active sort x-offset so hit-testing matches rendering position
        let candidates: Vec<_> = self
            .paths
            .iter()
            .flat_map(|path| {
                Self::path_to_hit_candidates(path, &self.viewport, self.active_sort_x_offset)
            })
            .collect();

        tracing::debug!("[hit_test_point] Found {} candidates", candidates.len());

        if let Some(first) = candidates.first() {
            tracing::debug!(
                "[hit_test_point] First candidate: pos=({}, {})",
                first.1.x,
                first.1.y
            );
        }

        // Find closest point in screen space
        let result = hit_test::find_closest(screen_pos, candidates.into_iter(), max_dist);

        if let Some(ref hit) = result {
            tracing::debug!(
                "[hit_test_point] Hit found: entity={:?}, distance={}",
                hit.entity,
                hit.distance
            );
        } else {
            tracing::debug!("[hit_test_point] No hit found");
        }

        result
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
    ) -> Option<(crate::path::SegmentInfo, f64)> {
        // Convert screen position to design space
        let mut design_pos = self.viewport.screen_to_design(screen_pos);

        // Adjust for active sort offset - subtract offset so coordinates match paths at (0,0)
        design_pos.x -= self.active_sort_x_offset;

        let closest_segment = Self::find_closest_segment(&self.paths, design_pos);

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

    /// Hit test for a component at screen coordinates
    ///
    /// Returns the EntityId of the component if the point is inside its filled area.
    /// Components are tested in reverse order so topmost components are hit first.
    pub fn hit_test_component(&self, screen_pos: Point) -> Option<crate::model::EntityId> {
        use kurbo::Shape;

        // Convert screen position to design space
        let mut design_pos = self.viewport.screen_to_design(screen_pos);

        // Adjust for active sort offset
        design_pos.x -= self.active_sort_x_offset;

        // Get workspace to resolve component base glyphs
        let workspace = self.workspace.as_ref()?;
        let workspace_guard = workspace.read().ok()?;

        // Test each component in reverse order (topmost first)
        for component in self.glyph.components.iter().rev() {
            // Look up the base glyph
            let base_glyph = workspace_guard.glyphs.get(&component.base)?;

            // Build the component's path with transform applied
            let mut component_path = kurbo::BezPath::new();
            for contour in &base_glyph.contours {
                let path = crate::path::Path::from_contour(contour);
                let transformed = component.transform * path.to_bezpath();
                component_path.extend(transformed);
            }

            // Check if point is inside the component's path
            // winding() returns non-zero for points inside a filled region
            if component_path.winding(design_pos) != 0 {
                return Some(component.id);
            }
        }

        None
    }

    // ===== PRIVATE HELPERS =====

    /// Calculate the bounding box of selected points
    fn calculate_selection_bbox(paths: &[Path], selection: &Selection) -> Option<(usize, Rect)> {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut count = 0;

        for path in paths.iter() {
            Self::collect_selected_points_from_path(
                path, selection, &mut min_x, &mut max_x, &mut min_y, &mut max_y, &mut count,
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
            Path::Cubic(cubic) => Box::new(cubic.points.iter()),
            Path::Quadratic(quadratic) => Box::new(quadratic.points.iter()),
            Path::Hyper(hyper) => Box::new(hyper.points.iter()),
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
    ///
    /// The offset_x parameter allows translating points in design space before
    /// converting to screen coordinates. This is used for active sorts in text
    /// buffers that aren't positioned at x=0.
    fn path_to_hit_candidates(
        path: &Path,
        viewport: &ViewPort,
        offset_x: f64,
    ) -> Vec<(crate::model::EntityId, Point, bool)> {
        match path {
            Path::Cubic(cubic) => cubic
                .points()
                .iter()
                .map(|pt| {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pt = viewport.to_screen(offset_point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
            Path::Quadratic(quadratic) => quadratic
                .points()
                .iter()
                .map(|pt| {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pt = viewport.to_screen(offset_point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
            Path::Hyper(hyper) => hyper
                .points()
                .iter()
                .map(|pt| {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pt = viewport.to_screen(offset_point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
        }
    }

    /// Find the closest segment to a design space point
    fn find_closest_segment(
        paths: &[Path],
        design_pos: kurbo::Point,
    ) -> Option<(crate::path::SegmentInfo, f64, f64)> {
        let mut closest: Option<(crate::path::SegmentInfo, f64, f64)> = None;

        for path in paths.iter() {
            Self::process_path_segments(path, design_pos, &mut closest);
        }
        closest
    }

    /// Process segments from a single path and update closest segment
    fn process_path_segments(
        path: &Path,
        design_pos: kurbo::Point,
        closest: &mut Option<(crate::path::SegmentInfo, f64, f64)>,
    ) {
        match path {
            Path::Cubic(cubic) => {
                Self::process_path_segment_iterator(cubic.iter_segments(), design_pos, closest);
            }
            Path::Quadratic(quadratic) => {
                Self::process_path_segment_iterator(quadratic.iter_segments(), design_pos, closest);
            }
            Path::Hyper(hyper) => {
                Self::process_path_segment_iterator(hyper.iter_segments(), design_pos, closest);
            }
        }
    }

    /// Process an iterator of segments and update closest segment
    fn process_path_segment_iterator<I>(
        segments: I,
        design_pos: kurbo::Point,
        closest: &mut Option<(crate::path::SegmentInfo, f64, f64)>,
    ) where
        I: Iterator<Item = crate::path::SegmentInfo>,
    {
        for segment_info in segments {
            let (t, dist_sq) = segment_info.segment.nearest(design_pos);
            Self::update_closest_segment(closest, segment_info, t, dist_sq);
        }
    }

    /// Update the closest segment if this one is closer
    fn update_closest_segment(
        closest: &mut Option<(crate::path::SegmentInfo, f64, f64)>,
        segment_info: crate::path::SegmentInfo,
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
}
