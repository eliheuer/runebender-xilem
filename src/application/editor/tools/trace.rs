// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Placing a picture behind a glyph and tracing it where it sits.
//!
//! Any glyph, in an OpenType source or a neural one, can have one picture behind it: a drawing or
//! a page of calligraphy to follow. It is placed on the canvas, sized by its height in font
//! units, shown with display adjustments, and traced in place: the outline lands on the ink as it
//! is shown. `runebender::formats::image_trace` owns the tracing. This module owns the placement,
//! the decoded picture the canvas draws, and the commands.

use crate::application::platform::dialogs;
use crate::application::view::canvas::editor::PlacedImage;
use crate::application::workspace::{Mode, Workspace};
use masonry::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use runebender::formats::image_trace::{PlacedTraceOptions, trace_image_placed};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How the picture is shown: display adjustments that never touch its file or its trace.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageAdjust {
    /// Added to every channel, from -1 to 1.
    pub brightness: f64,
    /// Spread of each channel around the middle; 1 leaves it, 0 is flat gray.
    pub contrast: f64,
    /// Color strength; 1 leaves it, 0 is gray, 2 doubles it.
    pub saturation: f64,
    /// How strongly the picture shows behind the outline, from 0 to 1.
    pub opacity: f64,
}

impl Default for ImageAdjust {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            opacity: 0.6,
        }
    }
}

impl ImageAdjust {
    /// One RGBA pixel, adjusted.
    fn apply(self, pixel: &mut [u8]) {
        let channel = |value: u8| f64::from(value) / 255.0;
        let [r, g, b] = [channel(pixel[0]), channel(pixel[1]), channel(pixel[2])];
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        for (index, value) in [r, g, b].into_iter().enumerate() {
            let value = luma + (value - luma) * self.saturation;
            let value = (value - 0.5) * self.contrast + 0.5 + self.brightness;
            pixel[index] = to_byte(value);
        }
        pixel[3] = to_byte(channel(pixel[3]) * self.opacity);
    }
}

fn to_byte(value: f64) -> u8 {
    // Clamped to 0..=255 first, so the cast cannot truncate.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to the byte range"
    )]
    let byte = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    byte
}

/// One decoded picture, kept so the canvas does not decode it on every frame.
pub(crate) struct DecodedImage {
    file_name: PathBuf,
    /// The encoded bytes this was decoded from, to notice a replaced file.
    source: Arc<[u8]>,
    /// The decoded pixels before any adjustment.
    original: Vec<u8>,
    /// The adjustment `data` was made with.
    adjust: ImageAdjust,
    data: ImageData,
}

impl Workspace {
    /// The picture attached to the open glyph, decoded for the canvas, with its placement.
    pub(crate) fn placed_image(&self) -> Option<PlacedImage> {
        if !self.show_background {
            return None;
        }
        let layer = self.session.current_layer()?;
        let image = layer.image()?;
        let source = self.font.project.source_id(self.font.active())?;
        let bytes = self
            .font
            .project
            .document_source_image(source, image.file_name())?;
        let mut cache = self
            .image_cache
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let adjust = self.image_adjust;
        let fresh = cache.as_ref().is_some_and(|decoded| {
            decoded.file_name == image.file_name() && Arc::ptr_eq(&decoded.source, &bytes)
        });
        if !fresh {
            let decoded = image::load_from_memory(&bytes).ok()?.to_rgba8();
            let (width, height) = decoded.dimensions();
            *cache = Some(DecodedImage {
                file_name: image.file_name().to_path_buf(),
                source: bytes,
                original: decoded.into_raw(),
                // differs from any real adjustment, so the pixels are made below
                adjust: ImageAdjust {
                    opacity: f64::NAN,
                    ..adjust
                },
                data: ImageData {
                    data: Blob::new(Arc::new(Vec::new())),
                    format: ImageFormat::Rgba8,
                    alpha_type: ImageAlphaType::Alpha,
                    width,
                    height,
                },
            });
        }
        let decoded = cache.as_mut()?;
        if decoded.adjust != adjust {
            let mut pixels = decoded.original.clone();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                adjust.apply(pixel);
            }
            decoded.data.data = Blob::new(Arc::new(pixels));
            decoded.adjust = adjust;
        }
        let data = cache.as_ref()?.data.clone();
        // A layer image has its origin at the lower left and y up; pixels run from the top.
        let flip = kurbo::Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, f64::from(data.height)]);
        Some(PlacedImage {
            to_glyph: image.transform() * flip,
            data,
        })
    }

    /// The placed picture's left edge, lower edge and height, in font units.
    pub(crate) fn image_frame(&self) -> Option<(f64, f64, f64)> {
        let image = self.placed_image_reference()?;
        let pixels = self.image_pixel_height()?;
        let [scale, _, _, _, x, y] = image.transform().as_coeffs();
        Some((x, y, scale * pixels))
    }

    fn placed_image_reference(&self) -> Option<runebender::font::LayerImage> {
        self.session.current_layer()?.image().cloned()
    }

    fn image_pixel_height(&self) -> Option<f64> {
        let image = self.placed_image_reference()?;
        let source = self.font.project.source_id(self.font.active())?;
        let bytes = self
            .font
            .project
            .document_source_image(source, image.file_name())?;
        let (_, height) = image::ImageReader::new(std::io::Cursor::new(&bytes[..]))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?;
        Some(f64::from(height))
    }

    /// Move the placed picture's left edge to `x` or its lower edge to `y`, or resize it to
    /// `height` font units, keeping its lower left corner where it is.
    ///
    /// Returns whether the placement changed.
    pub(crate) fn set_image_frame(
        &mut self,
        x: Option<f64>,
        y: Option<f64>,
        height: Option<f64>,
    ) -> bool {
        let (Some(image), Some(pixels)) =
            (self.placed_image_reference(), self.image_pixel_height())
        else {
            return false;
        };
        let [scale, _, _, _, old_x, old_y] = image.transform().as_coeffs();
        let scale = match height {
            Some(height) if height.is_finite() && height > 0.0 => height / pixels,
            Some(_) => return false,
            None => scale,
        };
        let x = x.filter(|x| x.is_finite()).unwrap_or(old_x);
        let y = y.filter(|y| y.is_finite()).unwrap_or(old_y);
        let before = self.image_frame();
        self.apply_op(move |session| {
            session.set_image_transform(kurbo::Affine::new([scale, 0.0, 0.0, scale, x, y]))
        });
        self.image_frame() != before
    }

    /// Store a picture in the source and attach it to the open glyph.
    ///
    /// The source keeps pictures as PNG, so any other format is converted. The picture is placed
    /// as tall as the font's ascender to descender span, with its lower edge on the descender.
    pub(crate) fn place_image_file(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        let format = image::guess_format(&bytes).map_err(|error| error.to_string())?;
        let decoded = image::load_from_memory(&bytes).map_err(|error| error.to_string())?;
        let (width, height) = (decoded.width(), decoded.height());
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".into());
        let bytes = if format == image::ImageFormat::Png {
            bytes
        } else {
            let mut png = Vec::new();
            decoded
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|error| error.to_string())?;
            png
        };
        let file_name = PathBuf::from(format!("{stem}.png"));
        let scale =
            ((self.font.ascender() - self.font.descender()) / f64::from(height).max(1.0)).max(1e-6);
        let placed = runebender::font::LayerImage::new(
            file_name.clone(),
            None,
            kurbo::Affine::new([scale, 0.0, 0.0, scale, 0.0, self.font.descender()]),
        )
        .map_err(|error| error.to_string())?;
        let source = self
            .font
            .project
            .source_id(self.font.active())
            .ok_or("the active source is unavailable")?;
        self.font
            .project
            .install_document_source_image(source, file_name.clone(), bytes)?;
        self.apply_op(move |session| session.set_image(Some(placed)));
        self.show_background = true;
        self.image_height_buf = None;
        self.image_x_buf = None;
        self.image_y_buf = None;
        self.note = format!("Placed {} · {width}×{height}px", file_name.display());
        Ok(())
    }

    /// Pick a raster image, store it in the UFO, and attach it to the glyph.
    pub(crate) fn command_place_image(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let Some(path) = dialogs::image(start) else {
            return;
        };
        if let Err(error) = self.place_image_file(&path) {
            self.note = format!("Place image: {error}");
        }
    }

    /// Add a trace of the open glyph's placed picture, where it sits, to its contours.
    pub(crate) fn command_trace_placed_image(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let traced = (|| {
            let image = self
                .placed_image_reference()
                .ok_or("place an image first")?;
            let source = self
                .font
                .project
                .source_id(self.font.active())
                .ok_or("the active source is unavailable")?;
            let bytes = self
                .font
                .project
                .document_source_image(source, image.file_name())
                .ok_or("the placed image is missing from the source")?;
            trace_image_placed(&bytes, image.transform(), self.trace_options())
        })();
        match traced {
            Ok(contours) => {
                // The trace joins the outline that is there, selected, so it can be moved or
                // deleted at once.
                let count = contours.len();
                self.apply_op(move |session| {
                    session.image_selected = false;
                    session.append_imported_contours(contours)
                });
                self.note = format!("Traced {count} contour(s)");
            }
            Err(error) => self.note = format!("Trace: {error}"),
        }
    }

    /// The trace settings the panel shows: its profile and invert switches, and the threshold
    /// field, where an empty field means automatic.
    pub(crate) fn trace_options(&self) -> PlacedTraceOptions {
        PlacedTraceOptions {
            threshold: self.trace_threshold_buf.trim().parse::<u8>().ok(),
            ..self.trace
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A white picture with one black block, as a JPEG: the source must store it as PNG.
    fn block_jpeg(path: &Path) {
        let mut picture = image::GrayImage::from_pixel(200, 100, image::Luma([255]));
        for y in 20..60 {
            for x in 40..120 {
                picture.put_pixel(x, y, image::Luma([0]));
            }
        }
        picture
            .save_with_format(path, image::ImageFormat::Jpeg)
            .unwrap();
    }

    #[test]
    fn a_placed_picture_is_stored_shown_resized_and_traced_in_place() {
        let directory = std::env::temp_dir().join(format!("xilem-trace-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("item.nufo");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("item"));
        font.save(&path).unwrap();
        let picture = directory.join("page.jpg");
        block_jpeg(&picture);

        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        assert!(app.session.neural, "a .nufo source opens in neural mode");
        app.place_image_file(&picture).unwrap();

        // stored as PNG, attached, and decoded for the canvas
        let layer_image = app
            .session
            .current_layer()
            .unwrap()
            .image()
            .cloned()
            .unwrap();
        assert_eq!(layer_image.file_name(), Path::new("page.png"));
        let shown = app
            .placed_image()
            .expect("the canvas has a picture to draw");
        assert_eq!((shown.data.width, shown.data.height), (200, 100));

        // 100 px tall, resized to 2000 units with its lower edge at -500
        // the default metrics placed it 1000 units tall on the descender
        assert_eq!(app.image_frame(), Some((0.0, -200.0, 1000.0)));
        assert!(
            !app.set_image_frame(None, None, Some(1000.0)),
            "the same height changes nothing"
        );
        assert!(app.set_image_frame(None, Some(-500.0), Some(2000.0)));
        let (_, y, height) = app.image_frame().unwrap();
        assert!((height - 2000.0).abs() < 1e-6 && (y + 500.0).abs() < 1e-6);

        // the block is x 40..120 px and, from the bottom, y 40..80 px: twenty units per pixel
        app.trace = PlacedTraceOptions {
            profile: runebender::formats::image_trace::TraceProfile::Clean,
            ..Default::default()
        };
        app.command_trace_placed_image();
        assert!(app.note.starts_with("Traced 1 contour"), "{}", app.note);
        let bounds = kurbo::Shape::bounding_box(&app.session.outline());
        let near = |a: f64, b: f64| (a - b).abs() <= 30.0;
        assert!(
            near(bounds.x0, 800.0) && near(bounds.x1, 2400.0),
            "{bounds:?}"
        );
        assert!(
            near(bounds.y0, 300.0) && near(bounds.y1, 1100.0),
            "{bounds:?}"
        );
        let traced_points = app.session.points().len();
        assert_eq!(
            app.session.selection.len(),
            traced_points,
            "the new trace is selected"
        );

        // tracing again keeps what is there: the outline now holds both traces
        app.command_trace_placed_image();
        assert_eq!(app.session.points().len(), 2 * traced_points);

        // the picture and its placement survive a save
        app.font.project.save().unwrap();
        let mut reopened = Workspace::open(&path).unwrap();
        reopened.open_glyph(reopened.font.index_of("item").unwrap());
        assert_eq!(reopened.image_frame(), app.image_frame());
        assert!(reopened.placed_image().is_some());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_dropped_picture_is_placed_behind_the_open_glyph() {
        let directory =
            std::env::temp_dir().join(format!("xilem-trace-drop-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("item.nufo");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("item"));
        font.save(&path).unwrap();
        let picture = directory.join("page.jpg");
        block_jpeg(&picture);

        let mut app = crate::application::workspace::AppState::open(Some(&path));
        // in the glyph grid a picture has nowhere to go
        app.file_dropped(picture.clone());
        let workspace = app.workspace.as_mut().unwrap();
        assert!(
            workspace.note.contains("Open a glyph"),
            "{}",
            workspace.note
        );
        workspace.open_glyph(workspace.font.index_of("item").unwrap());
        app.file_dropped(picture);
        let workspace = app.workspace.as_ref().unwrap();
        assert!(workspace.placed_image().is_some(), "{}", workspace.note);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_picture_locks_and_its_file_leaves_the_source_when_removed() {
        let directory =
            std::env::temp_dir().join(format!("xilem-trace-remove-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("item.nufo");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("item"));
        font.save(&path).unwrap();
        let picture = directory.join("page.jpg");
        block_jpeg(&picture);

        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.place_image_file(&picture).unwrap();
        let source = app.font.project.source_id(app.font.active()).unwrap();
        let stored = Path::new("page.png");
        assert!(
            app.font
                .project
                .document_source_image(source, stored)
                .is_some()
        );

        app.apply_op(|session| session.toggle_image_lock());
        assert!(app.session.image_locked());
        app.apply_op(|session| session.toggle_image_lock());
        assert!(!app.session.image_locked());

        app.apply_op(|session| session.remove_image());
        assert!(app.session.layer_image().is_none());
        assert!(
            app.font
                .project
                .document_source_image(source, stored)
                .is_none(),
            "an unused picture leaves the source"
        );
        app.font.project.save().unwrap();
        assert!(!path.join("images").join("page.png").exists());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn adjustments_change_only_how_the_picture_is_shown() {
        let mut pixel = [200, 100, 50, 255];
        ImageAdjust {
            opacity: 1.0,
            ..ImageAdjust::default()
        }
        .apply(&mut pixel);
        assert_eq!(
            pixel,
            [200, 100, 50, 255],
            "neutral settings change nothing"
        );

        let mut gray = [200, 100, 50, 255];
        ImageAdjust {
            saturation: 0.0,
            opacity: 1.0,
            ..ImageAdjust::default()
        }
        .apply(&mut gray);
        assert!(gray[0] == gray[1] && gray[1] == gray[2], "{gray:?}");

        let mut flat = [250, 5, 128, 200];
        ImageAdjust {
            contrast: 0.0,
            brightness: 0.25,
            opacity: 0.5,
            ..ImageAdjust::default()
        }
        .apply(&mut flat);
        assert_eq!(
            flat,
            [191, 191, 191, 100],
            "flat gray, raised, half as opaque"
        );
    }
}
