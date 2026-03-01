// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Background image support for tracing reference images in the glyph editor.
//!
//! Background images are session-only (not persisted to UFO). They allow users
//! to import scanned sketches or reference images and position them behind the
//! glyph outline for tracing.

use crate::theme;
use peniko::{Blob, ImageData, ImageFormat};
use std::path::{Path, PathBuf};

// ============================================================================
// RESIZE HANDLE
// ============================================================================

/// Which handle the user is dragging for resize.
///
/// Corner handles (circles) scale proportionally — the aspect ratio
/// is always preserved. Side handles (squares) scale freely along
/// one axis only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeHandle {
    // Corner handles — proportional scaling
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    // Side handles — free single-axis scaling
    Top,
    Bottom,
    Left,
    Right,
}

impl ResizeHandle {
    /// True for corner handles (proportional scale).
    pub fn is_corner(self) -> bool {
        matches!(
            self,
            Self::TopLeft
                | Self::TopRight
                | Self::BottomLeft
                | Self::BottomRight
        )
    }
}

// ============================================================================
// BACKGROUND IMAGE
// ============================================================================

/// A background reference image displayed behind the glyph in the editor.
///
/// The image lives in design space and moves/scales with the glyph.
/// It is session-only and not saved to the UFO file.
#[derive(Debug, Clone)]
pub struct BackgroundImage {
    /// Decoded RGBA8 pixel data for Vello rendering.
    pub image_data: ImageData,
    /// Original pixel width.
    pub width: u32,
    /// Original pixel height.
    pub height: u32,
    /// Position in design space (bottom-left corner of image).
    pub position: kurbo::Point,
    /// Horizontal scale factor.
    pub scale_x: f64,
    /// Vertical scale factor.
    pub scale_y: f64,
    /// Opacity from 0.0 (invisible) to 1.0 (fully opaque).
    pub opacity: f64,
    /// When true, the image cannot be moved or selected.
    pub locked: bool,
    /// Whether the image is currently selected.
    pub selected: bool,
    /// Original file path (kept for future img2bez tracing).
    #[allow(dead_code)]
    pub source_path: PathBuf,
}

impl BackgroundImage {
    /// Load a background image from a file path.
    ///
    /// Decodes the image to RGBA8 pixels and wraps them in a
    /// `peniko::ImageData` for Vello rendering. Returns an error
    /// if the file cannot be read or decoded.
    pub fn load(
        path: &Path,
        ascender: f64,
        descender: f64,
        glyph_width: f64,
    ) -> Result<Self, String> {
        let img = image::open(path)
            .map_err(|e| format!("Failed to load image: {e}"))?;
        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let pixels: Vec<u8> = rgba.into_raw();

        let blob = Blob::from(pixels);
        let image_data = ImageData {
            data: blob,
            format: ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width,
            height,
        };

        // Scale image to fit within the ascender-to-descender range
        let design_height = ascender - descender;
        let scale = design_height / height as f64;

        // Center horizontally within the glyph advance width
        let image_width_scaled = width as f64 * scale;
        let x = (glyph_width - image_width_scaled) / 2.0;
        let y = descender;

        Ok(Self {
            image_data,
            width,
            height,
            position: kurbo::Point::new(x, y),
            scale_x: scale,
            scale_y: scale,
            opacity: theme::background_image::DEFAULT_OPACITY,
            locked: false,
            selected: false,
            source_path: path.to_path_buf(),
        })
    }

    /// Scaled width in design units.
    pub fn scaled_width(&self) -> f64 {
        self.width as f64 * self.scale_x
    }

    /// Scaled height in design units.
    pub fn scaled_height(&self) -> f64 {
        self.height as f64 * self.scale_y
    }

    /// Return the bounding rectangle of the image in design space.
    pub fn bounds(&self) -> kurbo::Rect {
        kurbo::Rect::new(
            self.position.x,
            self.position.y,
            self.position.x + self.scaled_width(),
            self.position.y + self.scaled_height(),
        )
    }

    /// Hit-test a point in design space against the image bounds.
    pub fn contains(&self, point: kurbo::Point) -> bool {
        self.bounds().contains(point)
    }

    // ====================================================================
    // HANDLE POSITIONS
    // ====================================================================

    /// Return the four corner positions in design space
    /// (order: TL, TR, BL, BR).
    pub fn corner_positions(&self) -> [kurbo::Point; 4] {
        let b = self.bounds();
        [
            kurbo::Point::new(b.x0, b.y1), // top-left
            kurbo::Point::new(b.x1, b.y1), // top-right
            kurbo::Point::new(b.x0, b.y0), // bottom-left
            kurbo::Point::new(b.x1, b.y0), // bottom-right
        ]
    }

    /// Return the four side midpoint positions in design space
    /// (order: Top, Bottom, Left, Right).
    pub fn side_positions(&self) -> [kurbo::Point; 4] {
        let b = self.bounds();
        let cx = (b.x0 + b.x1) / 2.0;
        let cy = (b.y0 + b.y1) / 2.0;
        [
            kurbo::Point::new(cx, b.y1), // top
            kurbo::Point::new(cx, b.y0), // bottom
            kurbo::Point::new(b.x0, cy), // left
            kurbo::Point::new(b.x1, cy), // right
        ]
    }

    // ====================================================================
    // HIT TESTING
    // ====================================================================

    /// Hit-test against all 8 handles. Corner handles are checked
    /// first (they take priority at overlaps).
    pub fn hit_test_handle(
        &self,
        point: kurbo::Point,
        radius: f64,
    ) -> Option<ResizeHandle> {
        let radius_sq = radius * radius;

        let corners = self.corner_positions();
        let corner_handles = [
            ResizeHandle::TopLeft,
            ResizeHandle::TopRight,
            ResizeHandle::BottomLeft,
            ResizeHandle::BottomRight,
        ];
        for (pos, handle) in corners.iter().zip(corner_handles.iter())
        {
            let dx = point.x - pos.x;
            let dy = point.y - pos.y;
            if dx * dx + dy * dy <= radius_sq {
                return Some(*handle);
            }
        }

        let sides = self.side_positions();
        let side_handles = [
            ResizeHandle::Top,
            ResizeHandle::Bottom,
            ResizeHandle::Left,
            ResizeHandle::Right,
        ];
        for (pos, handle) in sides.iter().zip(side_handles.iter()) {
            let dx = point.x - pos.x;
            let dy = point.y - pos.y;
            if dx * dx + dy * dy <= radius_sq {
                return Some(*handle);
            }
        }

        None
    }

    /// Return the anchor edge or corner that stays fixed when
    /// dragging a given handle.
    pub fn anchor_for(&self, handle: ResizeHandle) -> kurbo::Point {
        let b = self.bounds();
        match handle {
            // Corners: opposite corner is the anchor
            ResizeHandle::TopLeft => {
                kurbo::Point::new(b.x1, b.y0)
            }
            ResizeHandle::TopRight => {
                kurbo::Point::new(b.x0, b.y0)
            }
            ResizeHandle::BottomLeft => {
                kurbo::Point::new(b.x1, b.y1)
            }
            ResizeHandle::BottomRight => {
                kurbo::Point::new(b.x0, b.y1)
            }
            // Sides: opposite edge midpoint is the anchor
            ResizeHandle::Top => {
                let cx = (b.x0 + b.x1) / 2.0;
                kurbo::Point::new(cx, b.y0)
            }
            ResizeHandle::Bottom => {
                let cx = (b.x0 + b.x1) / 2.0;
                kurbo::Point::new(cx, b.y1)
            }
            ResizeHandle::Left => {
                let cy = (b.y0 + b.y1) / 2.0;
                kurbo::Point::new(b.x1, cy)
            }
            ResizeHandle::Right => {
                let cy = (b.y0 + b.y1) / 2.0;
                kurbo::Point::new(b.x0, cy)
            }
        }
    }
}
