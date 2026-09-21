// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Draws a reproducible comparison of preview fitting and img2bez with sampled-field and exact-boundary inputs.

use std::fmt::Write as _;
use std::io::Write as _;

use kurbo::{Affine, BezPath, ParamCurve, Point, Shape, Vec2};
use runebender::formats::metaballs::{Metaball, MetaballGroup};
use runebender::outline::metaballs::{OutlineOptions, cubic_outline, field, preview};

fn ball(id: u32, x: f64, y: f64, radius: f64, stiffness: f64) -> Metaball {
    Metaball {
        id,
        x,
        y,
        radius,
        stiffness,
    }
}

// Compare the pinned img2bez pipeline using a finer grid and no font-unit rounding.
// The field is positive inside; image rows run downward and trace_sdf returns y-up paths.
#[allow(
    clippy::cast_possible_truncation,
    reason = "small fixed proof fixtures"
)]
fn image_fit(group: &MetaballGroup) -> Vec<BezPath> {
    let step = 0.5;
    let positive: Vec<_> = group.balls.iter().filter(|b| b.stiffness > 0.0).collect();
    let x0 = positive
        .iter()
        .map(|b| b.x - b.radius)
        .fold(f64::INFINITY, f64::min)
        - 2.0 * step;
    let y0 = positive
        .iter()
        .map(|b| b.y - b.radius)
        .fold(f64::INFINITY, f64::min)
        - 2.0 * step;
    let x1 = positive
        .iter()
        .map(|b| b.x + b.radius)
        .fold(f64::NEG_INFINITY, f64::max)
        + 2.0 * step;
    let y1 = positive
        .iter()
        .map(|b| b.y + b.radius)
        .fold(f64::NEG_INFINITY, f64::max)
        + 2.0 * step;
    let width = ((x1 - x0) / step).ceil() as usize + 1;
    let height = ((y1 - y0) / step).ceil() as usize + 1;
    let values: Vec<f32> = (0..width * height)
        .map(|i| {
            let point = Point::new(
                x0 + (i % width) as f64 * step,
                y0 + (height - 1 - i / width) as f64 * step,
            );
            (field(group, point) - group.threshold) as f32
        })
        .collect();
    let mut options = img2bez::TraceOptions::for_profile(img2bez::Profile::Clean)
        .with_grid(0)
        .with_em_height(height as f64 * step)
        .with_accuracy(0.25);
    options.min_contour_area = 0.0;
    options.smoothing = 0.0;
    options.cleanup_max_deviation = Some(0.25);
    img2bez::trace_sdf(width, height, &values, 1, &options)
        .unwrap()
        .to_bezpaths()
        .into_iter()
        .map(|p| Affine::translate((x0 - step * 0.5, y0 - step * 0.5)) * p)
        .collect()
}

fn discrepancy(group: &MetaballGroup, paths: &[BezPath]) -> f64 {
    let mut maximum: f64 = 0.0;
    for segment in paths.iter().flat_map(|path| path.segments()) {
        for i in 0..=100 {
            let point = segment.eval(f64::from(i) / 100.0);
            let dx = Vec2::new(0.001, 0.0);
            let dy = Vec2::new(0.0, 0.001);
            let gx = (field(group, point + dx) - field(group, point - dx)) / 0.002;
            let gy = (field(group, point + dy) - field(group, point - dy)) / 0.002;
            maximum = maximum.max((field(group, point) - group.threshold).abs() / gx.hypot(gy));
        }
    }
    maximum
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: metaball_conversion_proof <new.svg>")?;
    let cases = [
        ("Circle", vec![ball(1, 250.0, 700.0, 110.0, 2.0)]),
        (
            "Blended stem",
            vec![
                ball(1, 250.0, 160.0, 180.0, 2.0),
                ball(2, 250.0, 370.0, 180.0, 2.0),
            ],
        ),
        (
            "Unequal diagonal",
            vec![
                ball(1, 150.0, 150.0, 180.0, 2.0),
                ball(2, 290.0, 300.0, 140.0, 2.0),
            ],
        ),
        (
            "Counter",
            vec![
                ball(1, 250.0, 250.0, 220.0, 2.0),
                ball(2, 250.0, 250.0, 80.0, -2.0),
            ],
        ),
    ];
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1500" height="1610" viewBox="0 0 1500 1610"><rect width="1500" height="1610" fill="#faf9f6"/><g font-family="Arial,sans-serif" fill="#222"><text x="40" y="45" font-size="27">Metaballs → editable cubic outlines</text><text x="40" y="75" font-size="16">Actual output • blue: on-curve nodes • orange: control handles • same scale within each row</text>"##,
    );
    let titles = [
        "Previous / live preview",
        "img2bez · sampled field",
        "img2bez · exact boundary",
    ];
    for (column, title) in titles.iter().enumerate() {
        writeln!(
            svg,
            r#"<text x="{}" y="115" font-size="21">{title}</text>"#,
            column * 490 + 40
        )?;
    }
    for (row, (name, balls)) in cases.into_iter().enumerate() {
        let group = MetaballGroup {
            id: 1,
            threshold: 0.5,
            balls,
            links: Vec::new(),
        };
        let paths = [
            preview(&group, OutlineOptions::default())?,
            image_fit(&group),
            cubic_outline(&group, OutlineOptions::default())?,
        ];
        let bounds = paths[0]
            .iter()
            .map(Shape::bounding_box)
            .reduce(|a, b| a.union(b))
            .unwrap();
        let scale = (340.0 / bounds.width()).min(250.0 / bounds.height());
        for (column, paths) in paths.iter().enumerate() {
            let x = (column * 490 + 40) as f64;
            let y = (row * 350 + 150) as f64;
            let segments = paths.iter().map(|p| p.segments().count()).sum::<usize>();
            let error = discrepancy(&group, paths);
            println!(
                "{name}, {}: {segments} cubics; sampled normal discrepancy {error:.4}",
                titles[column]
            );
            writeln!(
                svg,
                r#"<text x="{x}" y="{y}" font-size="18">{name} · {segments} cubics</text><text x="{x}" y="{}" font-size="14">Estimated discrepancy: {error:.3} font units</text>"#,
                y + 22.0
            )?;
            let transform = Affine::translate((x + 215.0, y + 175.0))
                * Affine::scale_non_uniform(scale, -scale)
                * Affine::translate(-bounds.center().to_vec2());
            let mut combined = BezPath::new();
            for path in paths {
                combined.extend((transform * path).elements().iter().copied());
            }
            writeln!(
                svg,
                r##"<path d="{}" fill="#e5e7e5" stroke="#20252a" stroke-width="1.5"/>"##,
                combined.to_svg()
            )?;
            for c in combined.segments().map(|s| s.to_cubic()) {
                for (node, handle) in [(c.p0, c.p1), (c.p3, c.p2)] {
                    writeln!(
                        svg,
                        r##"<path d="M{} {} L{} {}" fill="none" stroke="#ca7b31" stroke-width="1"/><circle cx="{}" cy="{}" r="2.5" fill="#faf9f6" stroke="#ca7b31"/>"##,
                        node.x, node.y, handle.x, handle.y, handle.x, handle.y
                    )?;
                }
                writeln!(
                    svg,
                    r##"<circle cx="{}" cy="{}" r="3.2" fill="#3478b8"/>"##,
                    c.p0.x, c.p0.y
                )?;
            }
        }
    }
    svg.push_str(r#"<text x="40" y="1565" font-size="14">Discrepancy = max |F − threshold| / |gradient F| at 101 samples per cubic; an estimate, not a Hausdorff bound.</text><text x="40" y="1588" font-size="14">Exact boundary: grid 2, accuracy 0.25. Sampled field: Clean, grid 0.5, accuracy 0.25, cleanup limit 0.25, no smoothing or rounding.</text></g></svg>"#);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&out)?
        .write_all(svg.as_bytes())?;
    Ok(())
}
