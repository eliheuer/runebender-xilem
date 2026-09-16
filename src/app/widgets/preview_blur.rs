// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Cached Vello CPU proof blur for GPU backends without general filter support.

use masonry::imaging::record::Scene;
use masonry::imaging::render::ImageRenderer as _;
use masonry::imaging::{Filter, GroupRef, Painter};
use masonry::kurbo::{Affine, BezPath};
use masonry::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use std::cell::RefCell;
use std::sync::Arc;
use xilem::Color;

struct Cached {
    paths: Vec<BezPath>,
    transform: Affine,
    size: (u32, u32),
    sigma: f64,
    color: Color,
    image: ImageData,
}

thread_local! {
    static CACHE: RefCell<Option<Cached>> = const { RefCell::new(None) };
}

/// Rasterize only when the proof geometry, color, blur, or viewport changes.
pub(crate) fn render(
    paths: &[BezPath],
    transform: Affine,
    size: (u32, u32),
    sigma: f64,
    color: Color,
) -> Option<ImageData> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(hit) = cache.as_ref()
            && hit.paths == paths
            && hit.transform == transform
            && hit.size == size
            && hit.sigma == sigma
            && hit.color == color
        {
            return Some(hit.image.clone());
        }
        let mut scene = Scene::new();
        let mut painter = Painter::new(&mut scene);
        let filters = [Filter::blur(crate::view::render::px32(sigma))];
        painter.with_group(GroupRef::new().with_filters(&filters), |painter| {
            for path in paths {
                painter.fill(&(transform * path), color).draw();
            }
        });
        let mut renderer = imaging_vello_cpu::VelloCpuRenderer::new(1, 1);
        let rgba = renderer.render_source(&mut scene, size.0, size.1).ok()?;
        let image = ImageData {
            data: Blob::new(Arc::new(rgba.data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: rgba.width,
            height: rgba.height,
        };
        *cache = Some(Cached {
            paths: paths.to_vec(),
            transform,
            size,
            sigma,
            color,
            image: image.clone(),
        });
        Some(image)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::kurbo::{Rect, Shape};

    #[test]
    fn blur_spreads_coverage_and_can_be_composited() {
        let paths = [Rect::new(12.0, 12.0, 20.0, 20.0).to_path(0.1)];
        let image = render(&paths, Affine::IDENTITY, (32, 32), 2.0, Color::BLACK).unwrap();
        let mut scene = Scene::new();
        Painter::new(&mut scene).draw_image(&image, Affine::IDENTITY);
        let mut renderer = imaging_vello_cpu::VelloCpuRenderer::new(1, 1);
        let output = renderer.render_source(&mut scene, 32, 32).unwrap();
        let alpha = |x: usize, y: usize| output.data[(y * 32 + x) * 4 + 3];
        assert!(alpha(10, 16) > 0, "blur reaches beyond the original shape");
        assert!(
            alpha(16, 16) > alpha(10, 16),
            "center retains greater coverage"
        );
        assert_eq!(alpha(0, 0), 0, "distant background stays transparent");
    }
}
