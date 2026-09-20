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
                    request.respond(|call| app.call_live(call));
                }
            },
        ),
    )
}

impl Workspace {
    /// Dispatch through the same application refresh/history path for IPC and headless checks.
    pub(crate) fn call_live(
        &mut self,
        call: &runebender::document::agent::ToolCall,
    ) -> serde_json::Value {
        let mut result = self.handle_live(call);
        result["document_revision"] = serde_json::json!(self.font.project.document_revision());
        result["saved"] = serde_json::json!(false);
        result
    }

    fn handle_live(&mut self, call: &runebender::document::agent::ToolCall) -> serde_json::Value {
        use serde_json::json;
        if let Some(result) = self.call_agent_proof(call) {
            return result;
        }
        if let Some(result) = self.call_agent_edit(call) {
            return result;
        }
        if call.name == "editor_context" {
            if !call
                .arguments
                .as_object()
                .is_some_and(|args| args.is_empty())
            {
                return json!({"ok":false,"error":"editor_context takes no arguments", "error_code":"invalid_arguments"});
            }
            return self.live_context();
        }
        if matches!(
            call.name.as_str(),
            "proposal_install" | "experiment_apply" | "experiment_undo_apply"
        ) && self.session.gesture_in_progress()
        {
            return json!({"ok":false,"error":"finish the canvas gesture before installing", "error_code":"busy_gesture"});
        }
        let result =
            runebender::document::live::call(&mut self.font.project, &call.name, &call.arguments);
        if let Some(installed) = installed_in_active_source(&self.font.project, &result) {
            let edits = installed
                .iter()
                .filter_map(|name| self.font.active_layer_address(name))
                .map(|address| InstalledProposalEdit {
                    layer_history_depth: self.font.project.document_layer_history_depth(
                        &address,
                        runebender::document::history::HistoryDirection::Undo,
                    ),
                    address,
                })
                .collect::<Vec<_>>();
            self.ai.installed_order.extend(edits);
            self.after_font_change(&installed);
        }
        self.modified |= self.font.project.is_modified();
        self.refresh_proposals();
        result
    }

    fn live_context(&self) -> serde_json::Value {
        use crate::application::workspace::Mode;
        use serde_json::json;
        use sha2::{Digest as _, Sha256};

        let project = &self.font.project;
        let editing = matches!(self.mode, Mode::Editor(_));
        let glyph = editing
            .then(|| project.document_glyph(&self.session.glyph_name))
            .flatten();
        let address = editing
            .then(|| self.font.active_layer_address(&self.session.glyph_name))
            .flatten();
        let mut points = if editing {
            self.session
                .selection
                .iter()
                .map(|id| id.to_wire())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        points.sort();
        let mut disabled = self.text_features_disabled.iter().collect::<Vec<_>>();
        disabled.sort();
        let mut overview = self
            .multi_selected
            .iter()
            .filter_map(|index| self.font.glyphs.get(*index))
            .filter_map(|glyph| project.document_glyph(&glyph.name))
            .map(|glyph| glyph.id().to_wire())
            .collect::<Vec<_>>();
        overview.sort();
        let context = json!({
            "document_epoch": self.live.as_ref().map(|server| server.document_epoch()),
            "document_revision": project.document_revision(),
            "source_id": project.source_id(project.active).map(|source| source.0),
            "mode": match self.mode { Mode::Overview => "overview", Mode::Editor(_) => "editor", Mode::Nodes => "nodes" },
            "tab_id": self.tabs.get(self.active_tab).map(|tab| tab.text_context_id.to_string()),
            "glyph": glyph.map(|_| &self.session.glyph_name),
            "glyph_id": glyph.map(|glyph| glyph.id().to_wire()),
            "layer": address.map(|address| json!({"source_id":address.layer.source.0,"name":address.layer.name})),
            "selection": {
                "points": points,
                "anchor": editing.then_some(self.session.selected_anchor).flatten().map(|id| id.to_wire()),
                "component": editing.then_some(self.session.selected_component).flatten().map(|id| id.to_wire()),
                "overview_glyphs": overview,
            },
            "tool": format!("{:?}", self.tool).to_lowercase(),
            "busy_gesture": self.session.gesture_in_progress(),
            "text": {
                "editor": self.initial_text,
                "session_active": self.has_text_session,
                "preview": self.preview_text,
                "direction": self.text_dir.map(|direction| match direction {
                    runebender::text::buffer::TextDirection::LeftToRight => "ltr",
                    runebender::text::buffer::TextDirection::RightToLeft => "rtl",
                }).unwrap_or("auto"),
                "features_disabled": disabled,
                "script": self.text_script,
                "language": self.text_language,
                "caret": null,
                "selection_range": null,
            },
            "location": project.axes.iter().zip(&self.axis_values)
                .map(|(axis, value)| json!({"tag":axis.tag.as_ref(),"user":value})).collect::<Vec<_>>(),
        });
        let revision = format!("{:x}", Sha256::digest(context.to_string().as_bytes()));
        json!({
            "ok":true, "live":true, "saved":false,
            "document_revision":project.document_revision(),
            "context_revision":revision, "context":context,
            "capabilities":{
                "atomic_edits":true,
                "operation_receipts":true,
                "async_compiled_proofs":true,
                "proof_artifact_handles":true,
                "reusable_compiled_font_handles":false,
                "max_retained_proofs":super::live_proofs::MAX_SESSION_PROOFS,
                "edit_cancellation":true,
                "cancellation_identity":"document_epoch+actor+operation_key",
                "max_pending_live_requests":runebender::document::live_socket::MAX_PENDING_REQUESTS,
                "max_live_connections":runebender::document::live_socket::MAX_LIVE_CONNECTIONS,
                "cancellation_entries":runebender::document::live_socket::CANCELLATION_CAPACITY,
                "max_live_frame_bytes":runebender::document::live_socket::MAX_FRAME_BYTES,
                "max_agent_actors":super::live_edits::MAX_ACTORS,
                "receipts_per_actor":super::live_edits::RECEIPTS_PER_ACTOR,
                "application_context":true,
                "widget_text_ranges":false,
                "auxiliary_layer_canvas_selection":false,
                "context_revision_kind":"sha256-content",
                "identity_scope":"document-epoch",
                "read_state":"committed-project; active gesture draft excluded",
            },
        })
    }
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

    fn socket_call(
        app: &mut Workspace,
        name: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        use runebender::document::{agent::ToolCall, live_socket};
        use std::time::{Duration, Instant};
        let path = app
            .live
            .as_ref()
            .expect("fixture endpoint")
            .path()
            .to_path_buf();
        let call = ToolCall {
            name: name.into(),
            arguments,
        };
        let client = std::thread::spawn(move || live_socket::call(&path, &call).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(request) = app.live.as_ref().unwrap().try_recv() {
                request.respond(|call| app.call_live(call));
                break;
            }
            assert!(Instant::now() < deadline, "application mailbox deadline");
            std::thread::sleep(Duration::from_millis(5));
        }
        client.join().unwrap()
    }

    #[test]
    fn live_socket_context_unsaved_apply_and_editor_undo() {
        use crate::application::{font_model::FontModel, workspace::Mode};
        use serde_json::json;
        let path =
            std::env::temp_dir().join(format!("live-app-never-saved-{}.ufo", std::process::id()));
        assert!(!path.exists());
        let mut project = Project::new_font(path.clone());
        project
            .add_document_glyph("A", 400.0, Some(u32::from('A')))
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(412.0)?;
                Ok(())
            })
            .unwrap();
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.mode = Mode::Editor(app.font.index_of("A").unwrap());
        let context = socket_call(&mut app, "editor_context", json!({}));
        assert_eq!(context["context"]["glyph"], "A");
        assert_eq!(context["context"]["source_id"], source.0);
        assert_eq!(context["capabilities"]["widget_text_ranges"], false);
        assert!(context["context"]["text"]["caret"].is_null());
        let epoch = context["document_epoch"].clone();
        assert_eq!(context["context"]["document_epoch"], epoch);
        let read = socket_call(
            &mut app,
            "read_glyph",
            json!({"source":source.0,"glyph":"A","expected_document_epoch":epoch}),
        );
        assert_eq!(read["advance"], 412.0);
        assert_eq!(read["glyph_id"], context["context"]["glyph_id"]);
        assert_eq!(read["document_epoch"], epoch);
        let proposal = socket_call(
            &mut app,
            "propose_edits",
            json!({
                "source":source.0,"task":"socket-spacing","reason":"application integration",
                "expected_document_epoch":epoch,
                "edits":[{"glyph":"A","expected_revision":read["revision"],
                    "operations":[{"op":"set_width","width":430.0}]}]
            }),
        );
        assert_eq!(proposal["ok"], true, "{proposal}");
        let before_stale = app.font.project.document_revision();
        let stale = socket_call(
            &mut app,
            "proposal_install",
            json!({
                "source":source.0,"task":"socket-spacing","keep_structure":true,
                "authorization":"user-approved","expected_document_epoch":"another-document"
            }),
        );
        assert_eq!(stale["error_code"], "stale_document");
        assert_eq!(app.font.project.document_revision(), before_stale);
        let installed = socket_call(
            &mut app,
            "proposal_install",
            json!({
                "source":source.0,"task":"socket-spacing","keep_structure":true,
                "authorization":"user-approved","expected_document_epoch":epoch
            }),
        );
        assert_eq!(installed["ok"], true, "{installed}");
        assert_eq!(
            app.font.glyphs[app.font.index_of("A").unwrap()].advance,
            430.0
        );
        assert_eq!(app.session.advance(), 430.0);
        assert_eq!(app.ai.installed_order.len(), 1);
        app.undo_install();
        assert_eq!(app.session.advance(), 412.0);
        assert_eq!(
            app.font.glyphs[app.font.index_of("A").unwrap()].advance,
            412.0
        );
        assert!(app.ai.installed_order.is_empty());
        let repeated = socket_call(&mut app, "editor_context", json!({}));
        let stable = socket_call(&mut app, "editor_context", json!({}));
        assert_eq!(repeated["context_revision"], stable["context_revision"]);
        app.text_language = Some("he".into());
        let changed = socket_call(&mut app, "editor_context", json!({}));
        assert_ne!(changed["context_revision"], stable["context_revision"]);
        let mut other =
            Workspace::from_model(FontModel::from_project(Project::new_font(path.clone())))
                .unwrap();
        let other = socket_call(&mut other, "editor_context", json!({}));
        assert_ne!(other["document_epoch"], epoch);
        assert!(!path.exists(), "no live request saves the document");
    }

    #[test]
    fn context_revision_binds_identical_state_to_its_document_epoch() {
        use crate::application::font_model::FontModel;
        let path = std::env::temp_dir().join("live-epoch-never-saved.ufo");
        let mut app =
            Workspace::from_model(FontModel::from_project(Project::new_font(path))).unwrap();
        let before = app.live_context();
        // Replace only the endpoint lifetime, leaving every font and UI value identical.
        app.live = Some(runebender::document::live_socket::Server::start().unwrap());
        let after = app.live_context();
        assert_ne!(
            before["context"]["document_epoch"],
            after["context"]["document_epoch"]
        );
        assert_ne!(before["context_revision"], after["context_revision"]);
        assert_eq!(before["document_revision"], after["document_revision"]);
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
