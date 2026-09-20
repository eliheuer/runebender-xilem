// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Synthetic live document for testing MCP image delivery without opening a user font.
//! Run with `cargo run --example live_design_fixture`; connect to the printed socket.

#[cfg(unix)]
fn main() {
    use runebender::document::{live, live_socket::Server, project::Project};
    let mut project = Project::new_font("synthetic-not-saved.ufo".into());
    project
        .add_document_glyph("image_probe", 600.0, None)
        .unwrap();
    let source = project.source_id(0).unwrap();
    let layer = project.document_source(source).unwrap().default_layer();
    project
        .edit_document_layer("image_probe", &layer, |draft| {
            let (contour, _) = draft.start_contour(kurbo::Point::new(50.0, 0.0))?;
            draft.append_contour_segment(contour, None, kurbo::Point::new(300.0, 700.0), false)?;
            draft.append_contour_segment(contour, None, kurbo::Point::new(550.0, 0.0), false)?;
            draft.close_contour(contour, None)?;
            Ok(())
        })
        .unwrap();
    if let Some(path) = std::env::args().nth(1) {
        project.source_snapshot(source).unwrap().save(path).unwrap();
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
