// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Feature generation and composition commands.

use super::*;

/// Report generated mark features or write an include beside `features.fea`.
pub(in crate::application::cli) fn features_cmd(source: &Path, write: bool, json: bool) -> i32 {
    use runebender::text::features;
    if write
        && !source
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ufo"))
    {
        return fail(json, exit::USAGE, "features --write requires a UFO source");
    }
    let project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => {
            return fail(json, exit::USAGE, &format!("{}: {error}", source.display()));
        }
    };
    let mut sources = project.document_sources();
    let Some(selected) = sources.next() else {
        return fail(json, exit::USAGE, "the font has no source");
    };
    if sources.next().is_some() {
        return fail(
            json,
            exit::USAGE,
            "features requires one UFO source, not a variable project",
        );
    }
    let source_id = selected.id();
    let Some(generated) = features::generate_project(&project, source_id) else {
        return fail(
            json,
            exit::FAILED,
            "the source has no canonical feature inputs",
        );
    };
    let own_mark = features::defines_mark_features(
        project
            .document_feature_text(source_id)
            .expect("the selected source has canonical feature text"),
    );
    let written = if write {
        match features::write(source, &generated, true) {
            Ok((path, included)) => Some((path, included)),
            Err(e) => return fail(json, exit::FAILED, &e),
        }
    } else {
        None
    };
    if json {
        println!(
            "{}",
            json!({
                "ok": true,
                "classes": generated.classes,
                "marks": generated.marks,
                "bases": generated.bases,
                "stacked": generated.stacked,
                "empty": generated.is_empty(),
                "features_fea_defines_mark": own_mark,
                "written": written.as_ref().map(|(p, _)| p),
                "included": written.as_ref().map(|(_, i)| *i),
                "fea": if write { serde_json::Value::Null } else { json!(generated.fea) },
            })
        );
    } else {
        match &written {
            Some((path, included)) => {
                println!(
                    "{}: {} classes, {} marks, {} bases, {} stacked; include line {}",
                    path.display(),
                    generated.classes.len(),
                    generated.marks,
                    generated.bases,
                    generated.stacked,
                    if *included {
                        "present"
                    } else if own_mark {
                        "not added: features.fea defines mark features"
                    } else {
                        "not added"
                    }
                );
            }
            None => print!("{}", generated.fea),
        }
    }
    exit::OK
}

pub(in crate::application::cli) fn compose_cmd(
    source: &Path,
    glyphs: Option<&[String]>,
    write: bool,
    json: bool,
) -> i32 {
    let mut project = match Project::load(source) {
        Ok(project) => project,
        Err(error) => {
            return fail(json, exit::USAGE, &format!("{}: {error}", source.display()));
        }
    };
    let source_id = {
        let mut sources = project.document_sources();
        let Some(selected) = sources.next() else {
            return fail(json, exit::USAGE, "the font has no source");
        };
        if sources.next().is_some() {
            return fail(
                json,
                exit::USAGE,
                "compose requires one UFO source, not a variable project",
            );
        }
        selected.id()
    };
    let plan = match compose::plan_project(&project, source_id, glyphs) {
        Ok(plan) => plan,
        Err(error) => return fail(json, exit::FAILED, &format!("compose: {error}")),
    };
    let report = if write && !plan.replacements.is_empty() {
        match proposal::write_composition_project(&mut project, source_id, plan) {
            Ok(report) => report,
            Err(error) => return fail(json, exit::FAILED, &format!("compose: {error}")),
        }
    } else {
        plan.report
    };
    if write
        && report.proposal.is_some()
        && let Err(error) = project.save()
    {
        return fail(
            json,
            exit::FAILED,
            &format!("{}: {error}", source.display()),
        );
    }
    if json {
        println!(
            "{}",
            json!({
                "ok": true,
                "derived": report.derived,
                "proposed": report.proposed(),
                "skipped": report.skipped,
                "proposal": report.proposal,
            })
        );
    } else {
        for d in &report.derived {
            let parts: Vec<String> = d
                .components
                .iter()
                .map(|(n, x, y)| format!("{n}@{x:.0},{y:.0}"))
                .collect();
            println!(
                "{:<28} {:<10} {}{}",
                d.glyph,
                format!("{:?}", d.recipe.source).to_lowercase(),
                parts.join(" + "),
                if d.up_to_date { "  (up to date)" } else { "" }
            );
        }
        for (g, why) in &report.skipped {
            println!("{g:<28} skipped: {why}");
        }
        match &report.proposal {
            Some(p) => println!("proposal {}: {} glyphs", p.task, p.glyphs.len()),
            None if write => println!("nothing to propose"),
            None => {}
        }
    }
    exit::OK
}
