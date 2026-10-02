// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared project loading for focused headless font commands.

use super::*;

mod features;
mod inspection;
mod outlines;
mod proposals;

pub(super) use features::{compose_cmd, features_cmd};
pub(super) use inspection::{info, phrases, proof, proof_content};
pub(super) use outlines::{bolden, collapse_metaballs};
pub(super) use proposals::{
    find_font_ml, proposal_discard, proposal_install, proposal_list, propose,
};

/// Load one canonical source for a headless command.
fn open_project(path: &Path, json: bool) -> Result<Project, i32> {
    let project = Project::load(path)
        .map_err(|error| fail(json, exit::USAGE, &format!("{}: {error}", path.display())))?;
    if project.document_sources().count() != 1 {
        return Err(fail(
            json,
            exit::USAGE,
            "this command requires a single source; open the variable project in the editor",
        ));
    }
    Ok(project)
}

/// Save a canonical Project, reporting a write failure as such.
fn save_project(project: &mut Project, json: bool) -> Result<(), i32> {
    let path = project
        .document_sources()
        .next()
        .map(|source| source.path().to_path_buf())
        .unwrap_or_default();
    project
        .save()
        .map_err(|error| fail(json, exit::FAILED, &format!("{}: {error}", path.display())))
}

fn codepoints(codepoints: impl Iterator<Item = char>) -> Vec<String> {
    codepoints
        .map(|c| format!("U+{:04X}", u32::from(c)))
        .collect()
}
