// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Writes editable organic metaball fixtures and checks their silhouettes before cubic conversion.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use kurbo::{Affine, BezPath, Line, ParamCurve, Point, Rect, Shape};
use runebender::formats::metaballs::{
    Metaball, MetaballGroup, Metaballs, read_metaballs, write_metaballs,
};
use runebender::outline::metaballs::{OutlineOptions, cubic_outline, preview};

struct Fixture {
    name: &'static str,
    group: MetaballGroup,
    contours: usize,
    sweep: bool,
}

fn ball(id: u32, x: f64, y: f64, size: f64) -> Metaball {
    Metaball {
        id,
        x,
        y,
        radius: size / (1.0 - 0.25_f64.cbrt()).sqrt(),
        stiffness: 2.0,
    }
}

fn fixture(name: &'static str, balls: Vec<Metaball>, blend: f64, contours: usize) -> Fixture {
    Fixture {
        name,
        group: MetaballGroup {
            id: 1,
            threshold: 0.5,
            balls,
            links: Vec::new(),
            blend: Some(blend),
        },
        contours,
        sweep: false,
    }
}

fn fixtures() -> Vec<Fixture> {
    let equal = || vec![ball(1, 250.0, 400.0, 100.0), ball(2, 530.0, 400.0, 100.0)];
    let mut cases = vec![
        fixture("Equal-circle narrow hourglass", equal(), 0.25, 1),
        fixture(
            "Unequal pinched bridge: 100 / 60, distance 480",
            vec![ball(1, 180.0, 400.0, 100.0), ball(2, 660.0, 400.0, 60.0)],
            0.75,
            1,
        ),
        fixture(
            "Three-lobed junction",
            vec![
                ball(1, 310.0, 300.0, 90.0),
                ball(2, 590.0, 300.0, 90.0),
                ball(3, 450.0, 542.487_113, 90.0),
            ],
            0.9,
            1,
        ),
    ];
    // Every sweep row has exactly the same center positions and isolated radii.
    // The equal pair's connector onset is 0.1; 0.12 deliberately probes just after contact.
    for (name, blend, contours) in [
        ("Fixed-position sweep: separate", 0.0, 2),
        ("Fixed-position sweep: just after contact", 0.12, 1),
        ("Fixed-position sweep: narrow", 0.30, 1),
        ("Fixed-position sweep: broad", 0.85, 1),
    ] {
        let mut case = fixture(name, equal(), blend, contours);
        case.sweep = true;
        cases.push(case);
    }
    cases.push(fixture(
        "Bent / S-like arrangement",
        vec![
            ball(1, 180.0, 550.0, 70.0),
            ball(2, 360.0, 350.0, 50.0),
            ball(3, 540.0, 450.0, 80.0),
            ball(4, 720.0, 250.0, 60.0),
        ],
        0.7,
        1,
    ));
    cases.push(fixture(
        "Negative-space counter",
        vec![
            ball(1, 450.0, 400.0, 150.0),
            Metaball {
                stiffness: -2.0,
                ..ball(2, 450.0, 400.0, 50.0)
            },
        ],
        0.5,
        2,
    ));
    cases.push(fixture(
        "Three-lobed ring: natural central hole",
        vec![
            ball(1, 300.0, 300.0, 90.0),
            ball(2, 600.0, 300.0, 90.0),
            ball(3, 450.0, 559.807_621, 90.0),
        ],
        0.9,
        2,
    ));
    cases
}

fn combined(paths: &[BezPath]) -> BezPath {
    let mut result = BezPath::new();
    for path in paths {
        result.extend(path.elements().iter().copied());
    }
    result
}

fn section_width(path: &BezPath, x: f64) -> f64 {
    let line = Line::new((x, -1000.0), (x, 2000.0));
    let mut heights: Vec<_> = path
        .segments()
        .flat_map(|segment| {
            segment
                .intersect_line(line)
                .into_iter()
                .map(move |intersection| segment.eval(intersection.segment_t).y)
        })
        .collect();
    heights.sort_by(f64::total_cmp);
    match (heights.first(), heights.last()) {
        (Some(min), Some(max)) => max - min,
        _ => 0.0,
    }
}

fn source_bounds(group: &MetaballGroup) -> Rect {
    group
        .balls
        .iter()
        .filter(|ball| ball.stiffness > 0.0)
        .map(|ball| {
            let radius = ball.radius * (1.0 - 0.25_f64.cbrt()).sqrt();
            Rect::new(
                ball.x - radius,
                ball.y - radius,
                ball.x + radius,
                ball.y + radius,
            )
        })
        .reduce(|a, b| a.union(b))
        .expect("the fixture has positive circles")
}

fn check_lobes(case: &Fixture, path: &BezPath) -> Result<(), String> {
    let actual = path.bounding_box();
    let expected = source_bounds(&case.group);
    for (a, b) in [
        (actual.x0, expected.x0),
        (actual.y0, expected.y0),
        (actual.x1, expected.x1),
        (actual.y1, expected.y1),
    ] {
        if (a - b).abs() > 0.5 {
            return Err(format!(
                "{}: outer lobe bounds changed by {} units",
                case.name,
                (a - b).abs()
            ));
        }
    }
    if case.name == "Negative-space counter" && path.contains(Point::new(450.0, 400.0)) {
        return Err("the negative center must remain a hole".into());
    }
    if case.name == "Three-lobed ring: natural central hole"
        && path.contains(Point::new(450.0, 386.602_540))
    {
        return Err("the natural central hole must remain open".into());
    }
    Ok(())
}

fn draw(svg: &mut String, path: &BezPath, transform: Affine, nodes: bool) {
    let path = transform * path;
    writeln!(svg, r##"<path d="{}" fill="#444"/>"##, path.to_svg()).unwrap();
    if nodes {
        for segment in path.segments().map(|segment| segment.to_cubic()) {
            for (node, handle) in [(segment.p0, segment.p1), (segment.p3, segment.p2)] {
                writeln!(svg, r##"<path d="M{} {} L{} {}" stroke="#111"/><circle cx="{}" cy="{}" r="2" fill="#ddd" stroke="#111"/>"##,
                    node.x, node.y, handle.x, handle.y, handle.x, handle.y).unwrap();
            }
            writeln!(
                svg,
                r##"<circle cx="{}" cy="{}" r="3" fill="#eee" stroke="#111"/>"##,
                segment.p0.x, segment.p0.y
            )
            .unwrap();
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
        .ok_or("usage: metaball_organic_proof <new-output-directory>")?;
    let out = Path::new(&out);
    std::fs::create_dir(out)?;
    let mut font = norad::Font::new();
    font.font_info.family_name = Some("Organic Metaball Proof".into());
    font.font_info.style_name = Some("Regular".into());
    font.font_info.units_per_em = Some(1000.into());
    font.font_info.ascender = Some(800.0);
    font.font_info.descender = Some(-200.0);
    let mut svg = String::from(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1100" height="2720" viewBox="0 0 1100 2720"><rect width="1100" height="2720" fill="#b6b6b6"/><g font-family="sans-serif" fill="#111"><text x="30" y="35" font-size="23">Organic metaballs: fixed source circles, automatic curved blending</text><text x="30" y="65" font-size="15">Live silhouette</text><text x="580" y="65" font-size="15">img2bez conversion: nodes and handles</text>"##,
    );
    let mut report = String::from(
        "Organic metadata v3. Isolated radii in font units; no explicit links.\nThe fixed-position sweep changes only group Blend.\nChecks cover contour count, retained outer lobe bounds, a negative hole, increasing neck widths, and a nonuniform long bridge.\n",
    );
    let mut sweep_widths = Vec::new();
    for ((row, letter), case) in ('a'..='j').enumerate().zip(fixtures()) {
        let data = Metaballs {
            version: 3,
            groups: vec![case.group.clone()],
        };
        let mut glyph = norad::Glyph::new(&letter.to_string());
        glyph.width = 900.0;
        glyph.codepoints.insert(letter);
        write_metaballs(&mut glyph, &data)?;
        let paths = [
            preview(&case.group, OutlineOptions::default())?,
            cubic_outline(&case.group, OutlineOptions::default())?,
        ];
        let mut joined = Vec::new();
        for paths in &paths {
            if paths.len() != case.contours {
                return Err(format!(
                    "{}: expected {} contours, received {}",
                    case.name,
                    case.contours,
                    paths.len()
                )
                .into());
            }
            let path = combined(paths);
            check_lobes(&case, &path)?;
            joined.push(path);
        }
        if case.sweep {
            sweep_widths.push([
                section_width(&joined[0], 390.0),
                section_width(&joined[1], 390.0),
            ]);
        }
        let mut necks = Vec::new();
        if letter == 'b' {
            // All these sections lie strictly between the original circles.
            for path in &joined {
                let widths: Vec<_> = (0..=6)
                    .map(|i| section_width(path, 300.0 + f64::from(i) * 45.0))
                    .collect();
                let minimum = widths.iter().copied().fold(f64::INFINITY, f64::min);
                if minimum <= 0.0 || widths[0] <= minimum + 1.0 || widths[6] <= minimum + 1.0 {
                    return Err(format!(
                        "long unequal bridge must have a curved interior waist: {widths:?}"
                    )
                    .into());
                }
                necks.push(widths);
            }
        }
        let bounds = source_bounds(&case.group);
        let scale = (450.0 / bounds.width()).min(170.0 / bounds.height());
        let y = row as f64 * 260.0 + 105.0;
        let segments = paths[1]
            .iter()
            .map(|path| path.segments().count())
            .sum::<usize>();
        let rate = case.group.blend.expect("organic fixture");
        writeln!(
            svg,
            r#"<text x="30" y="{y}" font-size="17">{letter}: {} · Blend {rate:.2}</text><text x="580" y="{y}" font-size="15">{} contours · {segments} segments</text>"#,
            case.name, case.contours
        )?;
        for (column, path) in joined.iter().enumerate() {
            let transform = Affine::translate((column as f64 * 550.0 + 275.0, y + 125.0))
                * Affine::scale_non_uniform(scale, -scale)
                * Affine::translate(-bounds.center().to_vec2());
            draw(&mut svg, path, transform, column == 1);
        }
        writeln!(
            report,
            "{letter}: {}: Blend {rate:.2}, {} contours, {segments} converted segments; long-gap sections {necks:?}",
            case.name, case.contours
        )?;
        font.default_layer_mut().insert_glyph(glyph);
    }
    for column in 0..2 {
        if sweep_widths[0][column] != 0.0
            || !sweep_widths
                .windows(2)
                .all(|pair| pair[1][column] > pair[0][column])
        {
            return Err(format!(
                "fixed-position neck widths must increase from separate to broad: {sweep_widths:?}"
            )
            .into());
        }
    }
    writeln!(
        report,
        "Fixed-position neck widths [preview, converted]: {sweep_widths:?}"
    )?;
    let font_path = out.join("OrganicMetaballs.ufo");
    font.save(&font_path)?;
    let reopened = norad::Font::load(&font_path)?;
    for glyph in font.default_layer().iter() {
        let saved = reopened
            .default_layer()
            .get_glyph(glyph.name())
            .ok_or("saved glyph missing")?;
        if read_metaballs(glyph)? != read_metaballs(saved)? {
            return Err("organic metadata changed during UFO save/reopen".into());
        }
    }
    svg.push_str("</g></svg>");
    report.push_str("All source metadata survived UFO save/reopen exactly.\n");
    write_new(&out.join("organic-metaballs.svg"), &svg)?;
    write_new(&out.join("summary.txt"), &report)?;
    print!("{report}");
    println!("Proof directory: {}", out.display());
    Ok(())
}
