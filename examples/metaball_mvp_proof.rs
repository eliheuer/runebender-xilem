// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Writes disposable editable sources and a monochrome preview/conversion proof for metaball links.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use kurbo::{Affine, BezPath, Point, Shape};
use runebender::formats::metaballs::{
    Metaball, MetaballGroup, MetaballLink, Metaballs, read_metaballs, write_metaballs,
};
use runebender::outline::metaballs::{OutlineOptions, cubic_outline, preview};

fn ball(id: u32, x: f64, y: f64, size: f64) -> Metaball {
    Metaball {
        id,
        x,
        y,
        radius: size / (1.0 - 0.25_f64.cbrt()).sqrt(),
        stiffness: 2.0,
    }
}

fn link(id: u32, start: u32, end: u32, width: f64) -> MetaballLink {
    MetaballLink {
        id,
        start,
        end,
        width,
    }
}

fn cases() -> Vec<(&'static str, MetaballGroup, usize)> {
    let group = |balls, links| MetaballGroup {
        id: 1,
        threshold: 0.5,
        balls,
        links,
    };
    let pair = |separation| {
        // Match the existing support-180, strength-2 default-scale regression fixtures.
        let size = 180.0 * (1.0 - 0.25_f64.cbrt()).sqrt();
        group(
            vec![
                ball(1, 250.0, 400.0, size),
                ball(2, 250.0 + separation, 400.0, size),
            ],
            Vec::new(),
        )
    };
    vec![
        ("Separate", pair(280.0), 2),
        ("Near contact", pair(252.0), 1),
        ("Broad union", pair(200.0), 1),
        (
            "Unequal long bridge",
            group(
                vec![ball(1, 150.0, 400.0, 100.0), ball(2, 750.0, 400.0, 50.0)],
                vec![link(1, 1, 2, 24.0)],
            ),
            1,
        ),
        (
            "Chain",
            group(
                vec![
                    ball(1, 150.0, 350.0, 50.0),
                    ball(2, 350.0, 450.0, 50.0),
                    ball(3, 550.0, 350.0, 50.0),
                    ball(4, 750.0, 450.0, 50.0),
                ],
                vec![
                    link(1, 1, 2, 24.0),
                    link(2, 2, 3, 24.0),
                    link(3, 3, 4, 24.0),
                ],
            ),
            1,
        ),
        (
            "Branch",
            group(
                vec![
                    ball(1, 450.0, 400.0, 40.0),
                    ball(2, 450.0, 650.0, 60.0),
                    ball(3, 200.0, 200.0, 60.0),
                    ball(4, 700.0, 200.0, 60.0),
                ],
                vec![
                    link(1, 1, 2, 24.0),
                    link(2, 1, 3, 24.0),
                    link(3, 1, 4, 24.0),
                ],
            ),
            1,
        ),
        (
            "Counter",
            group(
                vec![
                    ball(1, 450.0, 400.0, 150.0),
                    Metaball {
                        stiffness: -2.0,
                        ..ball(2, 450.0, 400.0, 50.0)
                    },
                ],
                Vec::new(),
            ),
            2,
        ),
    ]
}

fn draw_paths(svg: &mut String, paths: &[BezPath], transform: Affine, nodes: bool) {
    let mut combined = BezPath::new();
    for path in paths {
        combined.extend((transform * path).elements().iter().copied());
    }
    writeln!(
        svg,
        r##"<path d="{}" fill="#777" stroke="#222" stroke-width="1.2"/>"##,
        combined.to_svg()
    )
    .expect("writing SVG to a String cannot fail");
    if nodes {
        for segment in combined.segments().map(|segment| segment.to_cubic()) {
            for (node, handle) in [(segment.p0, segment.p1), (segment.p3, segment.p2)] {
                writeln!(
                    svg,
                    r##"<path d="M{} {} L{} {}" stroke="#222"/><circle cx="{}" cy="{}" r="2" fill="#ddd" stroke="#222"/>"##,
                    node.x, node.y, handle.x, handle.y, handle.x, handle.y
                )
                .expect("writing SVG to a String cannot fail");
            }
            writeln!(
                svg,
                r##"<circle cx="{}" cy="{}" r="3" fill="#eee" stroke="#111"/>"##,
                segment.p0.x, segment.p0.y
            )
            .expect("writing SVG to a String cannot fail");
        }
    }
}

fn write_new(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?
        .write_all(contents.as_bytes())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .ok_or("usage: metaball_mvp_proof <new-output-directory>")?;
    let out = Path::new(&out);
    std::fs::create_dir(out)?;
    let mut font = norad::Font::new();
    font.font_info.family_name = Some("Metaball MVP Proof".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em = Some(1000.into());
    font.font_info.ascender = Some(800.0);
    font.font_info.descender = Some(-200.0);
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1100" height="2400" viewBox="0 0 1100 2400"><rect width="1100" height="2400" fill="#b6b6b6"/><g font-family="sans-serif" fill="#111"><text x="30" y="40" font-size="24">Metaball source and conversion fixtures</text><text x="30" y="70" font-size="16">Live preview + source centers</text><text x="580" y="70" font-size="16">img2bez cubics + nodes and handles</text>"##,
    );
    let mut report = String::from(
        "Fixed sources; grid 2 font units, fitting accuracy 0.25.\nNear contact is slightly connected: separation 252, support 180, strength 2, threshold 0.5.\nExact singular contact is not a smooth-outline fixture.\n",
    );
    for ((name, group, expected_contours), (row, letter)) in
        cases().into_iter().zip(('a'..='g').enumerate())
    {
        let data = Metaballs {
            version: if group.links.is_empty() { 1 } else { 2 },
            groups: vec![group.clone()],
        };
        let mut glyph = norad::Glyph::new(&letter.to_string());
        glyph.codepoints.insert(letter);
        glyph.width = 900.0;
        write_metaballs(&mut glyph, &data)?;
        let paths = [
            preview(&group, OutlineOptions::default())?,
            cubic_outline(&group, OutlineOptions::default())?,
        ];
        if paths.iter().any(|paths| paths.len() != expected_contours) {
            return Err(format!("{name}: expected {expected_contours} closed contours").into());
        }
        let bounds = paths
            .iter()
            .flatten()
            .map(Shape::bounding_box)
            .reduce(|a, b| a.union(b))
            .ok_or("fixture produced no geometry")?;
        let scale = (450.0 / bounds.width()).min(220.0 / bounds.height());
        let y = row as f64 * 320.0 + 110.0;
        writeln!(
            svg,
            r#"<text x="30" y="{y}" font-size="18">{letter}: {name}</text>"#
        )?;
        for (column, paths) in paths.iter().enumerate() {
            let x = column as f64 * 550.0 + 275.0;
            let transform = Affine::translate((x, y + 145.0))
                * Affine::scale_non_uniform(scale, -scale)
                * Affine::translate(-bounds.center().to_vec2());
            draw_paths(&mut svg, paths, transform, column == 1);
            if column == 0 {
                for ball in &group.balls {
                    let point = transform * Point::new(ball.x, ball.y);
                    writeln!(
                        svg,
                        r##"<circle cx="{}" cy="{}" r="4" fill="#ddd" stroke="#111"/>"##,
                        point.x, point.y
                    )?;
                }
            }
        }
        let segments = paths[1]
            .iter()
            .map(|path| path.segments().count())
            .sum::<usize>();
        writeln!(
            report,
            "{letter}: {name}: v{}, {} centers, {} links, {expected_contours} contours, {segments} converted segments",
            data.version,
            group.balls.len(),
            group.links.len()
        )?;
        font.default_layer_mut().insert_glyph(glyph);
    }
    svg.push_str("</g></svg>");
    let font_path = out.join("MetaballMvp.ufo");
    font.save(&font_path)?;
    let reopened = norad::Font::load(&font_path)?;
    for glyph in font.default_layer().iter() {
        let saved = reopened
            .default_layer()
            .get_glyph(glyph.name())
            .ok_or("saved fixture glyph is missing")?;
        if read_metaballs(glyph)? != read_metaballs(saved)? {
            return Err("saved metaball source changed".into());
        }
    }
    report.push_str("All fixture source metadata survived UFO save/reopen unchanged.\n");
    write_new(&out.join("metaballs.svg"), &svg)?;
    write_new(&out.join("summary.txt"), &report)?;
    print!("{report}");
    println!("Proof directory: {}", out.display());
    Ok(())
}
