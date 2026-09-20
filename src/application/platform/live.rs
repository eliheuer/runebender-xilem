// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Service the font engine's live document mailbox through Xilem application messages.

use crate::application::editor::tools::local_ai::InstalledProposalEdit;
use crate::application::workspace::Workspace;

fn installed_in_active_source(
    project: &runebender::document::project::Project,
    result: &serde_json::Value,
) -> Option<Vec<String>> {
    if result["root_changed"] != true {
        return None;
    }
    let changed_source = result["source_id"]
        .as_u64()
        .or_else(|| result["source"].as_u64())
        .and_then(|value| {
            usize::try_from(value)
                .ok()
                .map(runebender::document::variable::SourceId)
        })?;
    if project.source_id(project.active) != Some(changed_source) {
        return None;
    }
    serde_json::from_value(result["installed"]["installed"].clone()).ok()
}

/// Pumps the mailbox on the UI thread; socket workers never touch font data.
pub(crate) fn with_live<V: xilem::WidgetView<Workspace>>(
    view: V,
) -> impl xilem::WidgetView<Workspace> + use<V> {
    xilem::core::fork(
        view,
        xilem::view::task_raw(
            |proxy: xilem::core::MessageProxy<()>, _: &mut Workspace| async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                    if proxy.message(()).is_err() {
                        return;
                    }
                }
            },
            |app: &mut Workspace, ()| {
                let request = app.live.as_ref().and_then(|server| server.try_recv());
                if let Some(request) = request {
                    let mut installed: Vec<String> = Vec::new();
                    let mut root_changed = false;
                    request.respond(|call| {
                        if matches!(call.name.as_str(), "proposal_install" | "experiment_apply" | "experiment_undo_apply") && app.session.gesture_in_progress() {
                            return serde_json::json!({"ok":false,"error":"finish the canvas gesture before installing"});
                        }
                        let result = runebender::document::live::call(
                            &mut app.font.project,
                            &call.name,
                            &call.arguments,
                        );
                        if let Some(names) = installed_in_active_source(&app.font.project, &result) {
                            root_changed = true;
                            installed = names;
                        }
                        result
                    });
                    if root_changed {
                        let edits = installed
                            .iter()
                            .filter_map(|name| app.font.active_layer_address(name))
                            .map(|address| InstalledProposalEdit {
                                layer_history_depth: app.font.project.document_layer_history_depth(
                                    &address,
                                    runebender::document::history::HistoryDirection::Undo,
                                ),
                                address,
                            })
                            .collect::<Vec<_>>();
                        app.ai.installed_order.extend(edits);
                        app.after_font_change(&installed);
                    }
                    app.modified |= app.font.project.is_modified();
                    app.refresh_proposals();
                }
            },
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use runebender::document::project::{Project, SourceInput};
    use runebender::document::variable::SourceId;

    fn two_source_project() -> Project {
        let font = Project::new_font("synthetic.ufo".into())
            .encode_ufo_source(SourceId(0))
            .unwrap();
        let document = runebender::document::font_memory::designspace_from_str(
            r#"<designspace format="5.0"><axes><axis name="Weight" tag="wght" minimum="0" default="0" maximum="1"/></axes><sources><source filename="first.ufo"><location><dimension name="Weight" xvalue="0"/></location></source><source filename="second.ufo"><location><dimension name="Weight" xvalue="1"/></location></source></sources></designspace>"#,
        )
        .unwrap();
        Project::from_designspace(document, |path| {
            Ok(SourceInput::from_font(font.clone(), path.into()))
        })
        .unwrap()
    }

    #[test]
    fn live_root_changes_follow_stable_source_identity_after_reorder() {
        let mut project = two_source_project();
        let source = project.source_id(0).unwrap();
        assert!(project.move_source(source, 1).unwrap());
        project.active = 1;
        let result = serde_json::json!({
            "ok": true,
            "source": source.0,
            "root_changed": true,
            "installed": {"installed": ["A"]},
        });
        assert_eq!(
            installed_in_active_source(&project, &result),
            Some(vec!["A".to_owned()]),
            "a reordered active source must still trigger cache refresh and undo bookkeeping"
        );
        project.active = 0;
        assert_eq!(installed_in_active_source(&project, &result), None);
    }
}
