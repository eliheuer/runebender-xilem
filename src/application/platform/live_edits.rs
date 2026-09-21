// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Receipt-backed native live edits over the real Workspace and its application history.

use runebender::automation::agent::ToolCall;
use runebender::automation::agent_cancellation::{
    AgentCancellationError, AgentCancellationIdentity, AgentCancellationTerminal, AgentCommitClaim,
};
use runebender::automation::agent_edit::AgentEditRequest;
use runebender::automation::agent_session::{
    AgentOperationKey, AgentOperationOutcome, AgentOperationReceipt, AgentOperationRejection,
    AgentReceiptDisposition, AgentSession, AgentSessionError, AgentSessionMetadata,
};
use runebender::font::history::HistoryDirection;
use runebender::font::project::{DocumentEditObjectKind, EditHistoryGroupState, Project};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::workspace::Workspace;

pub(crate) const MAX_ACTORS: usize = 8;
pub(crate) const RECEIPTS_PER_ACTOR: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptRequest {
    expected_document_epoch: String,
    actor: String,
    operation_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryRequest {
    expected_document_epoch: String,
    actor: String,
    operation_key: String,
    authorization: String,
    direction: ReplayDirection,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReplayDirection {
    Undo,
    Redo,
}

impl Workspace {
    pub(crate) fn call_agent_edit(&mut self, call: &ToolCall) -> Option<Value> {
        Some(match call.name.as_str() {
            "agent_apply" => {
                match serde_json::from_value::<AgentEditRequest>(call.arguments.clone()) {
                    Ok(request) => self.apply_agent_edit(request),
                    Err(error) => failure("invalid_arguments", error.to_string()),
                }
            }
            "agent_receipt" => {
                match serde_json::from_value::<ReceiptRequest>(call.arguments.clone()) {
                    Ok(request) => match self.lookup_agent_receipt(
                        &request.expected_document_epoch,
                        &request.actor,
                        &request.operation_key,
                    ) {
                        Ok(receipt) => {
                            let mut result = receipt_result(&receipt, &self.font.project);
                            result["ok"] = json!(true);
                            result
                        }
                        Err(error) => error,
                    },
                    Err(error) => failure("invalid_arguments", error.to_string()),
                }
            }
            "agent_history" => {
                match serde_json::from_value::<HistoryRequest>(call.arguments.clone()) {
                    Ok(request) => self.replay_agent_edit(request),
                    Err(error) => failure("invalid_arguments", error.to_string()),
                }
            }
            _ => return None,
        })
    }

    fn validate_agent_epoch(&mut self, expected: &str) -> Result<(), Value> {
        let epoch = self
            .live
            .as_ref()
            .map(|server| server.document_epoch())
            .ok_or_else(|| {
                failure(
                    "session_unavailable",
                    "this workspace has no native endpoint",
                )
            })?;
        if expected != epoch {
            return Err(failure(
                "stale_document",
                "document epoch mismatch; reconnect explicitly",
            ));
        }
        if self
            .agent_sessions
            .values()
            .any(|session| session.metadata().document_epoch() != epoch)
        {
            self.agent_sessions.clear();
        }
        Ok(())
    }

    fn apply_agent_edit(&mut self, request: AgentEditRequest) -> Value {
        if let Err(error) = self.validate_agent_epoch(&request.expected_document_epoch) {
            return error;
        }
        let metadata =
            match AgentSessionMetadata::new(&request.expected_document_epoch, &request.actor) {
                Ok(metadata) => metadata,
                Err(error) => return failure("invalid_arguments", error.to_string()),
            };
        let key = match AgentOperationKey::new(&request.operation_key) {
            Ok(key) => key,
            Err(error) => return failure("invalid_arguments", error.to_string()),
        };
        let cancellation_identity = match AgentCancellationIdentity::new(
            &request.expected_document_epoch,
            &request.actor,
            &request.operation_key,
        ) {
            Ok(identity) => identity,
            Err(error) => return failure("invalid_arguments", error.to_string()),
        };
        let cancellations = self
            .live
            .as_ref()
            .expect("validated native endpoint remains available")
            .cancellations();
        let payload_digest = request.payload_digest();
        if request.authorization != "user-approved" {
            let _ = cancellations.release_unqueued(&cancellation_identity, payload_digest);
            return failure(
                "authorization_required",
                "use user-approved only within the user's granted edit authorization",
            );
        }
        // Direct Scripts/Nodes commands use this same adapter without visiting the socket queue.
        // Admission is idempotent: a transport reservation, including a prevented operation,
        // retains its state and payload identity rather than being replaced here.
        if let Err(error) = cancellations.admit(&cancellation_identity, payload_digest) {
            let code = match error {
                AgentCancellationError::PayloadMismatch => "payload_mismatch",
                AgentCancellationError::CapacityExhausted { .. } => "cancellation_capacity",
                _ => "cancellation_unavailable",
            };
            return failure(code, error.to_string());
        }
        if !self.agent_sessions.contains_key(&request.actor) {
            if self.agent_sessions.len() >= MAX_ACTORS {
                let _ = cancellations.release_unqueued(&cancellation_identity, payload_digest);
                return failure(
                    "actor_capacity",
                    "document actor capacity exhausted; existing receipts remain available",
                );
            }
            self.agent_sessions.insert(
                request.actor.clone(),
                AgentSession::new(metadata, RECEIPTS_PER_ACTOR)
                    .expect("fixed receipt capacity is within the engine limit"),
            );
        }
        let busy = self.session.gesture_in_progress();
        let session = self
            .agent_sessions
            .get_mut(&request.actor)
            .expect("actor ledger admitted above");
        let applied = session.apply_document_edit_with_precommit(
            &mut self.font.project,
            key,
            payload_digest,
            |project| {
                if busy {
                    return Err(AgentOperationRejection::InvalidRequest(
                        "finish the canvas gesture before a new edit".into(),
                    ));
                }
                request.stage(project)
            },
            || match cancellations.claim_commit(&cancellation_identity) {
                Ok(AgentCommitClaim::Claimed) => Ok(true),
                Ok(AgentCommitClaim::Prevented) => Ok(false),
                Ok(AgentCommitClaim::Unknown) => Err(AgentOperationRejection::InvalidRequest(
                    "operation cancellation state is unavailable; reconnect and retry with a new key"
                        .into(),
                )),
                Err(error) => Err(AgentOperationRejection::InvalidRequest(error.to_string())),
            },
        );
        let applied = match applied {
            Ok(applied) => applied,
            Err(error) => {
                if !matches!(error, AgentSessionError::PayloadMismatch { .. }) {
                    let _ = cancellations.release_unqueued(&cancellation_identity, payload_digest);
                }
                return match error {
                    AgentSessionError::PayloadMismatch { .. } => {
                        failure("payload_mismatch", error.to_string())
                    }
                    AgentSessionError::CapacityExhausted { .. } => {
                        failure("receipt_capacity", error.to_string())
                    }
                    _ => failure("invalid_arguments", error.to_string()),
                };
            }
        };
        if applied.disposition() == AgentReceiptDisposition::Recorded {
            let terminal = match applied.receipt().outcome() {
                AgentOperationOutcome::Committed { .. } => AgentCancellationTerminal::Committed,
                AgentOperationOutcome::Cancelled { .. } => AgentCancellationTerminal::Prevented,
                AgentOperationOutcome::Unchanged { .. }
                | AgentOperationOutcome::Rejected { .. } => AgentCancellationTerminal::Completed,
            };
            let _ = cancellations.finish(&cancellation_identity, terminal);
        }
        if applied.is_new_commit()
            && let AgentOperationOutcome::Committed {
                history_group,
                change,
                ..
            } = applied.receipt().outcome()
        {
            self.record_agent_group(*history_group, change);
        }
        let mut result = receipt_result(applied.receipt(), &self.font.project);
        result["replayed"] = json!(applied.disposition() == AgentReceiptDisposition::Replayed);
        result["root_changed"] = json!(applied.is_new_commit());
        result
    }

    fn lookup_agent_receipt(
        &mut self,
        epoch: &str,
        actor: &str,
        operation_key: &str,
    ) -> Result<AgentOperationReceipt, Value> {
        self.validate_agent_epoch(epoch)?;
        AgentSessionMetadata::new(epoch, actor)
            .map_err(|error| failure("invalid_arguments", error.to_string()))?;
        let key = AgentOperationKey::new(operation_key)
            .map_err(|error| failure("invalid_arguments", error.to_string()))?;
        self.agent_sessions
            .get(actor)
            .and_then(|session| session.receipt(&key))
            .cloned()
            .ok_or_else(|| {
                failure(
                    "unknown_operation",
                    "no receipt exists for this actor/key in this document epoch",
                )
            })
    }

    fn replay_agent_edit(&mut self, request: HistoryRequest) -> Value {
        if request.authorization != "user-approved" {
            return failure(
                "authorization_required",
                "history replay requires existing user authorization",
            );
        }
        let receipt = match self.lookup_agent_receipt(
            &request.expected_document_epoch,
            &request.actor,
            &request.operation_key,
        ) {
            Ok(receipt) => receipt,
            Err(error) => return error,
        };
        let Some(group) = receipt.history_group() else {
            return failure(
                "no_history_group",
                "the original operation did not commit a change",
            );
        };
        let direction = match request.direction {
            ReplayDirection::Undo => HistoryDirection::Undo,
            ReplayDirection::Redo => HistoryDirection::Redo,
        };
        match self.replay_agent_group(group, direction) {
            Ok(replay) => {
                let mut result = receipt_result(&receipt, &self.font.project);
                result["history_replayed"] = json!(true);
                result["replay_before_revision"] = json!(replay.before_revision);
                result["replay_after_revision"] = json!(replay.after_revision);
                result
            }
            Err(error) => failure("history_conflict", error),
        }
    }
}

fn receipt_result(receipt: &AgentOperationReceipt, project: &Project) -> Value {
    let outcome = match receipt.outcome() {
        AgentOperationOutcome::Committed {
            before_revision,
            after_revision,
            change,
            changed_objects,
            history_group,
        } => json!({
            "status":"committed", "before_revision":before_revision,"after_revision":after_revision,
            "history_group":history_group.to_wire(),
            "changed_layers":change.affected_layers().iter().map(|address| json!({"glyph":address.glyph,"source":address.layer.source.0,"layer":address.layer.name})).collect::<Vec<_>>(),
            "changed_objects":changed_objects.iter().map(|changed| {
                let mut result = json!({"glyph":changed.glyph,"glyph_id":changed.glyph_id.to_wire(),"source":changed.layer.source.0,"layer":changed.layer.name});
                match changed.object {
                    DocumentEditObjectKind::Width => result["kind"] = json!("width"),
                    DocumentEditObjectKind::Point(point_id) => {
                        result["kind"] = json!("point");
                        result["point_id"] = json!(point_id.to_wire());
                    }
                    DocumentEditObjectKind::Anchor(anchor_id) => {
                        result["kind"] = json!("anchor");
                        result["anchor_id"] = json!(anchor_id.to_wire());
                    }
                }
                result
            }).collect::<Vec<_>>()
        }),
        AgentOperationOutcome::Unchanged { revision } => {
            json!({"status":"unchanged","revision":revision,"changed_objects":[]})
        }
        AgentOperationOutcome::Cancelled { revision } => {
            json!({"status":"cancelled","cancellation":"prevented","revision":revision,
                "error":"operation cancelled before commit","error_code":"cancelled",
                "changed_objects":[]})
        }
        AgentOperationOutcome::Rejected {
            revision,
            rejection,
        } => {
            let message = match rejection {
                AgentOperationRejection::InvalidRequest(message) => message.clone(),
                AgentOperationRejection::Transaction(error) => error.to_string(),
            };
            json!({"status":"rejected","revision":revision,"error":message,"changed_objects":[]})
        }
    };
    let history_state = receipt.history_group().map(|group| {
        match project.document_edit_history_group_state(group) {
            Some(EditHistoryGroupState::Applied) => "applied",
            Some(EditHistoryGroupState::Undone) => "undone",
            None => "unavailable",
        }
    });
    json!({"ok":!matches!(outcome["status"].as_str(), Some("rejected" | "cancelled")), "saved":false, "history_state":history_state,
        "receipt":{"document_epoch":receipt.document_epoch(),"actor":receipt.actor(),"operation_key":receipt.operation_key().as_str(),"payload_sha256":receipt.payload_digest().to_hex(),"outcome":outcome}})
}

fn failure(code: &str, error: impl Into<String>) -> Value {
    json!({"ok":false,"error_code":code,"error":error.into(),"saved":false})
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::application::font_model::FontModel;
    use runebender::automation::live_socket;
    use runebender::font::edit_batch;
    use runebender::font::project::SourceInput;
    use runebender::font::variable::SourceId;

    fn project() -> Project {
        let path = std::env::temp_dir().join("agent-transaction-never-saved.ufo");
        let mut project = Project::new_font(path);
        for name in ["A", "B", "C"] {
            project.add_document_glyph(name, 400.0, None).unwrap();
        }
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        for name in ["A", "B", "C"] {
            project
                .edit_document_layer(name, &layer, |draft| {
                    draft.set_width(400.0)?;
                    Ok(())
                })
                .unwrap();
        }
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(20.0, 0.0, 300.0, 700.0), false)?;
                draft.add_anchor("top".into(), kurbo::Point::new(160.0, 720.0))?;
                Ok(())
            })
            .unwrap();
        project
    }

    fn workspace(project: Project) -> Workspace {
        let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
        app.open_glyph(app.font.index_of("B").unwrap());
        app.new_tab();
        app.open_glyph(app.font.index_of("A").unwrap());
        app
    }

    fn guard(app: &Workspace, source: usize, glyph: &str) -> Value {
        let layer_id = app
            .font
            .project
            .document_source(SourceId(source))
            .unwrap()
            .default_layer();
        let layer = app.font.project.document_layer(glyph, &layer_id).unwrap();
        json!({"glyph":glyph,"glyph_id":app.font.project.document_glyph(glyph).unwrap().id().to_wire(),"layer":layer_id.name,"expected_revision":edit_batch::canonical_glyph_revision(layer).unwrap()})
    }

    fn request(app: &Workspace, key: &str, source: usize, glyphs: &[(&str, f64)]) -> Value {
        json!({"expected_document_epoch":app.live.as_ref().unwrap().document_epoch(),
            "actor":"test-agent","operation_key":key,"authorization":"user-approved","source":source,
            "history_name":"Agent spacing batch","edits":glyphs.iter().map(|(glyph,width)| json!({"target":guard(app,source,glyph),"operations":[{"op":"set_width","width":width}]})).collect::<Vec<_>>()})
    }

    fn identity(payload: &Value) -> Value {
        json!({"expected_document_epoch":payload["expected_document_epoch"],"actor":payload["actor"],"operation_key":payload["operation_key"]})
    }

    fn dispatch_pending(app: &mut Workspace) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pending) = app.live.as_ref().unwrap().try_recv() {
                pending.respond(|call| app.call_live(call));
                return;
            }
            assert!(
                Instant::now() < deadline,
                "live transaction mailbox deadline"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn call(app: &mut Workspace, name: &str, arguments: Value) -> Value {
        let endpoint = app.live.as_ref().unwrap().path().to_owned();
        let request = ToolCall {
            name: name.into(),
            arguments,
        };
        let client = std::thread::spawn(move || live_socket::call(&endpoint, &request).unwrap());
        dispatch_pending(app);
        client.join().unwrap()
    }

    fn endpoint_call(app: &Workspace, name: &str, arguments: Value) -> Value {
        live_socket::call(
            app.live.as_ref().unwrap().path(),
            &ToolCall {
                name: name.into(),
                arguments,
            },
        )
        .unwrap()
    }

    fn replay(app: &mut Workspace, payload: &Value, direction: &str) -> Value {
        let mut args = identity(payload);
        args["authorization"] = json!("user-approved");
        args["direction"] = json!(direction);
        call(app, "agent_history", args)
    }

    fn width(app: &Workspace, source: usize, glyph: &str) -> f64 {
        let layer = app
            .font
            .project
            .document_source(SourceId(source))
            .unwrap()
            .default_layer();
        app.font
            .project
            .document_layer(glyph, &layer)
            .unwrap()
            .width()
    }

    #[test]
    fn direct_application_apply_shares_reservations_receipts_and_ordinary_undo() {
        let mut app = workspace(project());
        let payload = request(&app, "script-apply", 0, &[("A", 430.0)]);
        let tool = ToolCall {
            name: "agent_apply".into(),
            arguments: payload,
        };
        let applied = app.call_agent_edit(&tool).unwrap();
        assert_eq!(applied["receipt"]["outcome"]["status"], "committed");
        assert_eq!(width(&app, 0, "A"), 430.0);
        assert_eq!(app.metadata_undo.len(), 1);
        app.undo_active_edit(false);
        assert_eq!(width(&app, 0, "A"), 400.0);
        let retried = app.call_agent_edit(&tool).unwrap();
        assert_eq!(retried["replayed"], true);
        assert_eq!(retried["receipt"], applied["receipt"]);
        assert_eq!(width(&app, 0, "A"), 400.0);

        let pending = request(&app, "cancel-script-apply", 0, &[("A", 440.0)]);
        assert_eq!(
            endpoint_call(&app, "agent_reserve", pending.clone())["ok"],
            true
        );
        assert_eq!(
            endpoint_call(&app, "agent_cancel", identity(&pending))["cancellation_status"],
            "prevented"
        );
        let cancelled = app
            .call_agent_edit(&ToolCall {
                name: "agent_apply".into(),
                arguments: pending,
            })
            .unwrap();
        assert_eq!(cancelled["receipt"]["outcome"]["status"], "cancelled");
        assert_eq!(width(&app, 0, "A"), 400.0);
    }

    #[test]
    fn lost_response_retry_refreshes_once_and_shares_ordinary_targeted_history() {
        let mut app = workspace(project());
        let payload = request(&app, "lost-response", 0, &[("A", 430.0), ("B", 450.0)]);
        let revision = app.font.project.document_revision();
        let mut client = UnixStream::connect(app.live.as_ref().unwrap().path()).unwrap();
        writeln!(
            client,
            "{}",
            serde_json::to_string(&ToolCall {
                name: "agent_apply".into(),
                arguments: payload.clone()
            })
            .unwrap()
        )
        .unwrap();
        drop(client); // Commit after the caller has lost its response channel.
        dispatch_pending(&mut app);
        assert_eq!(app.font.project.document_revision(), revision + 1);
        assert_eq!(app.metadata_undo.len(), 1);
        assert_eq!(app.session.advance(), 430.0);
        assert_eq!(
            app.tabs
                .iter()
                .find(|tab| tab.session.glyph_name == "B")
                .unwrap()
                .session
                .advance(),
            450.0
        );
        let session = app.session.clone();
        let cells = app.cells.clone();
        let receipt = call(&mut app, "agent_receipt", identity(&payload));
        assert_eq!(receipt["history_state"], "applied");
        let retry = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["receipt"], receipt["receipt"]);
        assert_eq!(app.font.project.document_revision(), revision + 1);
        assert!(std::sync::Arc::ptr_eq(&session, &app.session));
        assert!(std::sync::Arc::ptr_eq(&cells, &app.cells));
        let mut conflicting = payload.clone();
        conflicting["edits"][0]["operations"][0]["width"] = json!(999.0);
        assert_eq!(
            endpoint_call(&app, "agent_apply", conflicting)["error_code"],
            "payload_mismatch"
        );
        let cancelled = live_socket::call(
            app.live.as_ref().unwrap().path(),
            &ToolCall {
                name: "agent_cancel".into(),
                arguments: identity(&payload),
            },
        )
        .unwrap();
        assert_eq!(cancelled["cancellation_status"], "committed");
        assert_eq!(width(&app, 0, "A"), 430.0);
        app.undo_active_edit(false);
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert_eq!(width(&app, 0, "B"), 400.0);
        assert_eq!(app.metadata_redo.len(), 1);
        assert_eq!(replay(&mut app, &payload, "undo")["ok"], false);
        let retry = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(retry["receipt"], receipt["receipt"]);
        assert_eq!(retry["history_state"], "undone");
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert_eq!(replay(&mut app, &payload, "redo")["ok"], true);
        assert_eq!(replay(&mut app, &payload, "undo")["ok"], true);
        let revision = app.font.project.document_revision();
        app.undo_active_edit(false);
        assert_eq!(
            app.font.project.document_revision(),
            revision,
            "ordinary undo cannot undo the group twice"
        );
        app.undo_active_edit(true);
        assert_eq!(app.session.advance(), 430.0);
        assert_eq!(app.metadata_undo.len(), 1);
        assert!(
            !std::env::temp_dir()
                .join("agent-transaction-never-saved.ufo")
                .exists()
        );
    }

    #[test]
    fn queued_cancel_records_terminal_receipt_and_exact_retry_never_edits() {
        let mut app = workspace(project());
        let payload = request(&app, "cancel-queued", 0, &[("A", 430.0)]);
        let identity = identity(&payload);
        let endpoint = app.live.as_ref().unwrap().path().to_owned();
        let apply_payload = payload.clone();
        let apply_endpoint = endpoint.clone();
        let apply = std::thread::spawn(move || {
            live_socket::call(
                &apply_endpoint,
                &ToolCall {
                    name: "agent_apply".into(),
                    arguments: apply_payload,
                },
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let pending = loop {
            if let Some(pending) = app.live.as_ref().unwrap().try_recv() {
                break pending;
            }
            assert!(Instant::now() < deadline, "apply did not enter mailbox");
            std::thread::sleep(Duration::from_millis(5));
        };

        let cancelled = live_socket::call(
            &endpoint,
            &ToolCall {
                name: "agent_cancel".into(),
                arguments: identity.clone(),
            },
        )
        .unwrap();
        assert_eq!(cancelled["cancellation_status"], "prevented");
        let revision = app.font.project.document_revision();
        pending.respond(|call| app.call_live(call));
        let result = apply.join().unwrap();
        assert_eq!(result["receipt"]["outcome"]["status"], "cancelled");
        assert_eq!(result["receipt"]["outcome"]["changed_objects"], json!([]));
        assert_eq!(result["root_changed"], false);
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert!(app.metadata_undo.is_empty());

        let retry = call(&mut app, "agent_apply", payload);
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["receipt"], result["receipt"]);
        assert_eq!(width(&app, 0, "A"), 400.0);
        let receipt = call(&mut app, "agent_receipt", identity.clone());
        assert_eq!(receipt["receipt"]["outcome"]["status"], "cancelled");

        let mut other_actor = identity;
        other_actor["actor"] = json!("another-actor");
        assert_eq!(
            live_socket::call(
                &endpoint,
                &ToolCall {
                    name: "agent_cancel".into(),
                    arguments: other_actor,
                },
            )
            .unwrap()["cancellation_status"],
            "unknown"
        );
    }

    #[test]
    fn reserved_cancel_records_one_receipt_and_rejects_changed_payload_retry() {
        let mut app = workspace(project());
        let payload = request(&app, "cancel-before-mailbox", 0, &[("A", 430.0)]);
        let endpoint = app.live.as_ref().unwrap().path().to_owned();
        let reserved = live_socket::call(
            &endpoint,
            &ToolCall {
                name: "agent_reserve".into(),
                arguments: payload.clone(),
            },
        )
        .unwrap();
        assert_eq!(reserved["reservation_status"], "new");
        let cancelled = live_socket::call(
            &endpoint,
            &ToolCall {
                name: "agent_cancel".into(),
                arguments: identity(&payload),
            },
        )
        .unwrap();
        assert_eq!(cancelled["cancellation_status"], "prevented");

        let revision = app.font.project.document_revision();
        let recorded = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(recorded["receipt"]["outcome"]["status"], "cancelled");
        assert_eq!(recorded["replayed"], false);
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert!(app.metadata_undo.is_empty());

        let replayed = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(replayed["replayed"], true);
        assert_eq!(replayed["receipt"], recorded["receipt"]);
        let mut conflicting = payload;
        conflicting["edits"][0]["operations"][0]["width"] = json!(999.0);
        assert_eq!(
            endpoint_call(&app, "agent_apply", conflicting)["error_code"],
            "payload_mismatch"
        );
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(width(&app, 0, "A"), 400.0);
    }

    #[test]
    fn stale_dependency_invalid_late_operation_and_payload_reuse_never_partially_apply() {
        let mut app = workspace(project());
        let mut payload = request(
            &app,
            "bad-third-operation",
            0,
            &[("A", 430.0), ("B", 450.0)],
        );
        payload["edits"][1]["operations"]
            .as_array_mut()
            .unwrap()
            .push(json!({"op":"set_point","point_id":"missing","x":1.0,"y":2.0}));
        let revision = app.font.project.document_revision();
        let failed = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(failed["receipt"]["outcome"]["status"], "rejected");
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert_eq!(width(&app, 0, "B"), 400.0);
        assert!(app.metadata_undo.is_empty());
        let repeat = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(repeat["receipt"], failed["receipt"]);
        payload["edits"][1]["operations"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert_eq!(
            endpoint_call(&app, "agent_apply", payload)["error_code"],
            "payload_mismatch"
        );

        let mut stale = request(&app, "stale-dependency", 0, &[("A", 430.0)]);
        stale["reads"] = json!([guard(&app, 0, "C")]);
        let layer = app
            .font
            .project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        app.font
            .project
            .edit_document_layer("C", &layer, |draft| {
                draft.set_width(900.0)?;
                Ok(())
            })
            .unwrap();
        let revision = app.font.project.document_revision();
        let stale = call(&mut app, "agent_apply", stale);
        assert_eq!(stale["ok"], false);
        assert_eq!(stale["receipt"]["outcome"]["status"], "rejected");
        assert_eq!(stale["receipt"]["outcome"]["changed_objects"], json!([]));
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(width(&app, 0, "A"), 400.0);
    }

    #[test]
    fn point_anchor_ids_and_history_conflicts_preserve_unrelated_edits() {
        let mut app = workspace(project());
        let address = app.font.active_layer_address("A").unwrap();
        let layer = app
            .font
            .project
            .document_layer("A", &address.layer)
            .unwrap();
        let point = layer
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .id();
        let anchor = layer.anchors().next().unwrap().id();
        let mut payload = request(&app, "move-objects", 0, &[("A", 430.0)]);
        payload["edits"][0]["operations"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"op":"set_point","point_id":point.to_wire(),"x":25.0,"y":5.0}),
                json!({"op":"set_anchor","anchor_id":anchor.to_wire(),"x":170.0,"y":725.0}),
            ]);
        let applied = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(applied["ok"], true);
        assert_eq!(
            applied["receipt"]["outcome"]["changed_objects"],
            json!([
                {"glyph":"A","glyph_id":app.font.project.document_glyph("A").unwrap().id().to_wire(),"source":0,"layer":address.layer.name,"kind":"width"},
                {"glyph":"A","glyph_id":app.font.project.document_glyph("A").unwrap().id().to_wire(),"source":0,"layer":address.layer.name,"kind":"point","point_id":point.to_wire()},
                {"glyph":"A","glyph_id":app.font.project.document_glyph("A").unwrap().id().to_wire(),"source":0,"layer":address.layer.name,"kind":"anchor","anchor_id":anchor.to_wire()}
            ])
        );
        assert_eq!(
            call(&mut app, "agent_receipt", identity(&payload))["receipt"]["outcome"]["changed_objects"],
            applied["receipt"]["outcome"]["changed_objects"]
        );
        let layer = app
            .font
            .project
            .document_layer("A", &address.layer)
            .unwrap();
        assert_eq!(
            layer
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .id(),
            point
        );
        assert_eq!(
            layer
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .position(),
            kurbo::Point::new(25.0, 5.0)
        );
        assert_eq!(
            layer
                .anchors()
                .find(|candidate| candidate.id() == anchor)
                .unwrap()
                .position(),
            kurbo::Point::new(170.0, 725.0)
        );
        app.font
            .project
            .edit_document_layer("C", &address.layer, |draft| {
                draft.set_width(900.0)?;
                Ok(())
            })
            .unwrap();
        assert_eq!(replay(&mut app, &payload, "undo")["ok"], true);
        assert_eq!(width(&app, 0, "C"), 900.0);
        assert_eq!(replay(&mut app, &payload, "redo")["ok"], true);
        app.font
            .project
            .edit_document_layer("A", &address.layer, |draft| {
                draft.set_width(470.0)?;
                Ok(())
            })
            .unwrap();
        let revision = app.font.project.document_revision();
        assert_eq!(
            replay(&mut app, &payload, "undo")["error_code"],
            "history_conflict"
        );
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(width(&app, 0, "A"), 470.0);
        assert_eq!(width(&app, 0, "C"), 900.0);
    }

    #[test]
    fn inactive_reordered_source_is_explicit_and_refreshes_when_selected() {
        let font = project().encode_ufo_source(SourceId(0)).unwrap();
        let document=runebender::font::font_memory::designspace_from_str(r#"<designspace format="5.0"><axes><axis name="Weight" tag="wght" minimum="0" default="0" maximum="1"/></axes><sources><source filename="first.ufo"><location><dimension name="Weight" xvalue="0"/></location></source><source filename="second.ufo"><location><dimension name="Weight" xvalue="1"/></location></source></sources></designspace>"#).unwrap();
        let mut project = Project::from_designspace(document, |path| {
            Ok(SourceInput::from_font(font.clone(), path.into()))
        })
        .unwrap();
        project.move_source(SourceId(1), 0).unwrap();
        project.active = 1;
        let mut app = workspace(project);
        let payload = request(&app, "inactive-source", 1, &[("A", 510.0)]);
        app.open_glyph(app.font.index_of("B").unwrap());
        assert_eq!(call(&mut app, "agent_apply", payload.clone())["ok"], true);
        assert_eq!(app.session.glyph_name, "B");
        assert_eq!(app.session.advance(), 400.0);
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert_eq!(width(&app, 1, "A"), 510.0);
        app.set_master(0);
        app.open_glyph(app.font.index_of("A").unwrap());
        assert_eq!(app.session.advance(), 510.0);
        app.undo_active_edit(false);
        assert_eq!(app.session.advance(), 400.0);
        assert_eq!(
            call(&mut app, "agent_receipt", identity(&payload))["history_state"],
            "undone"
        );
    }

    #[test]
    fn strict_arguments_epoch_and_capacity_fail_before_mutation() {
        let mut app = workspace(project());
        let payload = request(&app, "capacity", 0, &[("A", 430.0)]);
        for field in ["expected_document_epoch", "authorization", "source"] {
            let mut invalid = payload.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert_eq!(call(&mut app, "agent_apply", invalid)["ok"], false);
        }
        let mut invalid = payload.clone();
        invalid["branch"] = json!("implicit-branch");
        assert_eq!(
            call(&mut app, "agent_apply", invalid)["error_code"],
            "invalid_arguments"
        );
        let mut invalid = payload.clone();
        invalid["expected_document_epoch"] = json!("old");
        assert_eq!(
            call(&mut app, "agent_apply", invalid)["error_code"],
            "stale_document"
        );
        assert!(app.agent_sessions.is_empty());
        let metadata =
            AgentSessionMetadata::new(app.live.as_ref().unwrap().document_epoch(), "test-agent")
                .unwrap();
        app.agent_sessions
            .insert("test-agent".into(), AgentSession::new(metadata, 1).unwrap());
        assert_eq!(call(&mut app, "agent_apply", payload.clone())["ok"], true);
        let next = request(&app, "full", 0, &[("A", 460.0)]);
        let revision = app.font.project.document_revision();
        assert_eq!(
            call(&mut app, "agent_apply", next)["error_code"],
            "receipt_capacity"
        );
        assert_eq!(app.font.project.document_revision(), revision);
        for index in 1..MAX_ACTORS {
            let mut unchanged = request(&app, "unchanged", 0, &[("A", 430.0)]);
            unchanged["actor"] = json!(format!("actor-{index}"));
            let result = call(&mut app, "agent_apply", unchanged);
            assert_eq!(result["receipt"]["outcome"]["status"], "unchanged");
            assert_eq!(result["root_changed"], false);
        }
        let mut overflow = request(&app, "actor-overflow", 0, &[("A", 460.0)]);
        overflow["actor"] = json!("one-too-many");
        assert_eq!(
            call(&mut app, "agent_apply", overflow)["error_code"],
            "actor_capacity"
        );
        assert_eq!(app.font.project.document_revision(), revision);
        assert_eq!(app.metadata_undo.len(), 1);
        let mut fresh = workspace(project());
        let mut lookup = identity(&payload);
        lookup["expected_document_epoch"] = json!(fresh.live.as_ref().unwrap().document_epoch());
        assert_eq!(
            call(&mut fresh, "agent_receipt", lookup)["error_code"],
            "unknown_operation"
        );
    }

    #[test]
    fn overview_history_keeps_older_and_later_human_batches_in_order() {
        use crate::application::workspace::Mode;
        let mut app = workspace(project());
        app.mode = Mode::Overview;
        app.selected = app.font.index_of("C");
        app.overview_set_advance("450".into());
        let payload = request(&app, "overview-order", 0, &[("A", 430.0)]);
        assert_eq!(call(&mut app, "agent_apply", payload)["ok"], true);
        app.overview_set_advance("470".into());
        app.selected = app.font.index_of("A");
        app.undo_active_edit(false);
        assert_eq!(width(&app, 0, "C"), 450.0);
        assert_eq!(width(&app, 0, "A"), 430.0);
        app.undo_active_edit(false);
        assert_eq!(width(&app, 0, "A"), 400.0);
        assert_eq!(width(&app, 0, "C"), 450.0);
        app.undo_active_edit(true);
        assert_eq!(width(&app, 0, "A"), 430.0);
        assert_eq!(width(&app, 0, "C"), 450.0);
        app.undo_active_edit(true);
        assert_eq!(width(&app, 0, "C"), 470.0);
    }

    #[test]
    fn active_gesture_blocks_new_mutations_but_allows_receipt_replay() {
        let mut app = workspace(project());
        let payload = request(&app, "before-gesture", 0, &[("A", 430.0)]);
        let installed = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(installed["ok"], true);
        std::sync::Arc::make_mut(&mut app.session).begin_point_drag();
        assert!(app.session.gesture_in_progress());
        let revision = app.font.project.document_revision();
        let retry = call(&mut app, "agent_apply", payload.clone());
        assert_eq!(retry["receipt"], installed["receipt"]);
        assert_eq!(retry["replayed"], true);
        assert!(app.session.gesture_in_progress());
        let pending = request(&app, "during-gesture", 0, &[("A", 460.0)]);
        let rejected = call(&mut app, "agent_apply", pending);
        assert_eq!(rejected["receipt"]["outcome"]["status"], "rejected");
        assert_eq!(rejected["receipt"]["outcome"]["changed_objects"], json!([]));
        assert_eq!(replay(&mut app, &payload, "undo")["ok"], false);
        assert_eq!(app.font.project.document_revision(), revision);
        assert!(app.session.gesture_in_progress());
    }

    #[test]
    fn auxiliary_layer_group_is_undoable_without_changing_foreground() {
        let mut project = project();
        let foreground = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let auxiliary = project
            .add_glyph_layer("A", &foreground, "agent-review")
            .unwrap();
        let mut app = workspace(project);
        let read = call(
            &mut app,
            "read_glyph",
            json!({"source":0,"glyph":"A","layer":"agent-review"}),
        );
        let mut payload = request(&app, "auxiliary-edit", 0, &[("A", 470.0)]);
        payload["edits"][0]["target"] = json!({"glyph":"A","glyph_id":read["glyph_id"],"layer":read["layer"],"expected_revision":read["revision"]});
        assert_eq!(call(&mut app, "agent_apply", payload.clone())["ok"], true);
        assert_eq!(app.session.advance(), 400.0);
        assert_eq!(
            app.font
                .project
                .document_layer("A", &auxiliary)
                .unwrap()
                .width(),
            470.0
        );
        assert!(app.can_metadata_history_step(false));
        app.undo_active_edit(false);
        assert_eq!(
            app.font
                .project
                .document_layer("A", &auxiliary)
                .unwrap()
                .width(),
            400.0
        );
        assert_eq!(app.session.advance(), 400.0);
        assert_eq!(replay(&mut app, &payload, "redo")["ok"], true);
        assert_eq!(
            app.font
                .project
                .document_layer("A", &auxiliary)
                .unwrap()
                .width(),
            470.0
        );
        assert_eq!(app.session.advance(), 400.0);
    }
}
