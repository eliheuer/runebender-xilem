// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Synthetic live document for testing MCP image delivery without opening a user font.
//! Run with `cargo run --example live_design_fixture`; connect to the printed socket.

#[cfg(unix)]
fn main() {
    use runebender_core::document::{live, live_socket::Server, project::Project};
    let mut project = Project::new_font("synthetic-not-saved.ufo".into());
    project.masters[0].add_glyph("image_probe", 600.0).unwrap();
    let glyph = project.masters[0]
        .font
        .get_glyph_mut("image_probe")
        .unwrap();
    glyph.contours.push(norad::Contour::new(
        [(50.0, 0.0), (300.0, 700.0), (550.0, 0.0)]
            .into_iter()
            .map(|(x, y)| norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None))
            .collect(),
        None,
    ));
    if let Some(path) = std::env::args().nth(1) {
        project.masters[0].font.save(path).unwrap();
        return;
    }
    let server = Server::start().unwrap();
    println!("{}", server.path().display());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    while std::time::Instant::now() < deadline {
        if let Some(request) = server.try_recv() {
            request.respond(|call| live::call(&mut project, &call.name, &call.arguments));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[cfg(not(unix))]
fn main() {
    eprintln!("The live editor transport requires Unix sockets.");
}
