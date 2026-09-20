// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Service the font engine's live document mailbox through Xilem application messages.

use crate::application::editor::tools::local_ai::InstalledProposalEdit;
use crate::application::workspace::Workspace;

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
                        if result["root_changed"] == true
                            && result["master"].as_u64() == Some(app.font.project.active as u64)
                        {
                            root_changed = true;
                            installed =
                                serde_json::from_value(result["installed"]["installed"].clone())
                                    .unwrap_or_default();
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
