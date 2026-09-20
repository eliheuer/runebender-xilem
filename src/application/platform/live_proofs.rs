// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native proof artifact lifetimes and exact document binding.
//!
//! One process-wide worker survives Workspace replacements, so closing/reloading a font
//! cannot accumulate uninterruptible compiler threads. Sessions retain bounded handles only.
//! Dropping a session cancels queued jobs and abandons running results for later collection.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use runebender::document::agent::ToolCall;
use runebender::document::agent_proof::{
    ProofHandleRequest, ProofStartRequest, ProofStatusRequest,
};
use runebender::document::compiled_proof;
use runebender::document::proof_jobs::{
    ProofJobCancelOutcome, ProofJobHandle, ProofJobLineage, ProofJobOutcome, ProofJobQueue,
    ProofJobRequest, ProofJobStatus,
};
use serde_json::{Value, json};

use crate::application::workspace::Workspace;

pub(crate) const MAX_SESSION_PROOFS: usize = 8;
const MAX_PNG_BYTES: usize = 5 * 1024 * 1024;

static SERVICE: OnceLock<Mutex<ProofService>> = OnceLock::new();

struct ProofService {
    queue: ProofJobQueue,
    abandoned: BTreeSet<ProofJobHandle>,
}

impl ProofService {
    fn collect(&mut self) {
        self.abandoned.retain(|handle| !self.queue.discard(*handle));
    }
}

fn service() -> &'static Mutex<ProofService> {
    SERVICE.get_or_init(|| {
        Mutex::new(ProofService {
            queue: ProofJobQueue::new(16, 32).expect("fixed proof queue limits are valid"),
            abandoned: BTreeSet::new(),
        })
    })
}

struct RetainedProof {
    handle: ProofJobHandle,
    request: ProofStartRequest,
}

/// Per-document retry identities, with no mutable font or compiled byte ownership.
#[derive(Default)]
pub(crate) struct LiveProofSession {
    jobs: BTreeMap<String, RetainedProof>,
}

impl Drop for LiveProofSession {
    fn drop(&mut self) {
        if self.jobs.is_empty() {
            return;
        }
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        for record in self.jobs.values() {
            service.queue.cancel(record.handle);
            if !service.queue.discard(record.handle) {
                service.abandoned.insert(record.handle);
            }
        }
        service.collect();
    }
}

fn failure(code: &str, message: impl ToString) -> Value {
    json!({"ok":false,"error_code":code,"error":message.to_string(),"root_changed":false})
}

impl Workspace {
    pub(crate) fn call_agent_proof(&mut self, call: &ToolCall) -> Option<Value> {
        Some(match call.name.as_str() {
            "proof_start" => match serde_json::from_value(call.arguments.clone()) {
                Ok(request) => self.start_proof(request),
                Err(error) => failure("invalid_arguments", error),
            },
            "proof_status" => {
                match serde_json::from_value::<ProofStatusRequest>(call.arguments.clone()) {
                    Ok(request) => self.proof_action(
                        &request.expected_document_epoch,
                        &request.proof_id,
                        "status",
                        request.include_image,
                    ),
                    Err(error) => failure("invalid_arguments", error),
                }
            }
            "proof_cancel" | "proof_release" => {
                match serde_json::from_value::<ProofHandleRequest>(call.arguments.clone()) {
                    Ok(request) => self.proof_action(
                        &request.expected_document_epoch,
                        &request.proof_id,
                        &call.name,
                        false,
                    ),
                    Err(error) => failure("invalid_arguments", error),
                }
            }
            _ => return None,
        })
    }

    fn check_proof_epoch(&self, expected: &str) -> Result<(), Value> {
        let Some(server) = self.live.as_ref() else {
            return Err(failure(
                "session_unavailable",
                "workspace has no native endpoint",
            ));
        };
        if server.document_epoch() != expected {
            return Err(failure(
                "stale_document",
                "reconnect to the intended document epoch",
            ));
        }
        Ok(())
    }

    fn start_proof(&mut self, request: ProofStartRequest) -> Value {
        if let Err(error) = self.check_proof_epoch(&request.expected_document_epoch) {
            return error;
        }
        if request.operation_key.is_empty() || request.operation_key.len() > 128 {
            return failure(
                "invalid_arguments",
                "operation_key must contain 1 to 128 UTF-8 bytes",
            );
        }
        if let Err(error) = request.recipe.validate() {
            return failure("invalid_arguments", error);
        }
        // A retry remains valid after the live revision changes; it never recaptures.
        if let Some(record) = self.proof_jobs.jobs.get(&request.operation_key) {
            if record.request != request {
                return failure(
                    "operation_key_conflict",
                    "retained proof key has a different payload",
                );
            }
            return json!({"ok":true,"proof_id":record.handle.get().to_string(),"replayed":true,
                "captured_document_epoch":request.expected_document_epoch,
                "captured_document_revision":request.expected_document_revision,"root_changed":false});
        }
        if request.expected_document_revision != self.font.project.document_revision() {
            return failure(
                "stale_revision",
                "read the current document revision before capturing",
            );
        }
        if self.session.gesture_in_progress() {
            return failure(
                "busy_gesture",
                "finish the canvas gesture before capturing a proof",
            );
        }
        if request.recipe.normalized_location.len() > 64
            || request.recipe.normalized_location.len() != self.font.project.axes.len()
        {
            return failure(
                "invalid_arguments",
                "provide one normalized coordinate per document axis (maximum 64)",
            );
        }
        if self.proof_jobs.jobs.len() >= MAX_SESSION_PROOFS {
            return failure(
                "proof_capacity",
                "release a terminal proof before starting another",
            );
        }
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        service.collect();
        let input = match compiled_proof::capture(&self.font.project) {
            Ok(input) => input,
            Err(error) => return failure("proof_capture_failed", error),
        };
        let handle = match service.queue.submit(ProofJobRequest {
            lineage: ProofJobLineage {
                document_epoch: request.expected_document_epoch.clone(),
                document_revision: request.expected_document_revision,
            },
            input,
            recipe: request.recipe.clone(),
        }) {
            Ok(handle) => handle,
            Err(error) => return failure("proof_unavailable", format!("{error:?}")),
        };
        let result = json!({"ok":true,"proof_id":handle.get().to_string(),"replayed":false,
            "captured_document_epoch":request.expected_document_epoch,
            "captured_document_revision":request.expected_document_revision,"root_changed":false});
        self.proof_jobs.jobs.insert(
            request.operation_key.clone(),
            RetainedProof { handle, request },
        );
        result
    }

    fn proof_action(&mut self, epoch: &str, id: &str, action: &str, include_image: bool) -> Value {
        if let Err(error) = self.check_proof_epoch(epoch) {
            return error;
        }
        let Some((key, record)) = self
            .proof_jobs
            .jobs
            .iter()
            .find(|(_, record)| record.handle.get().to_string() == id)
        else {
            return failure("unknown_proof", "proof is not retained in this document");
        };
        let handle = record.handle;
        let key = key.clone();
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        service.collect();
        if action == "proof_release" {
            if !service.queue.discard(handle) {
                return failure(
                    "proof_busy",
                    "only terminal proofs can be released; cancel queued work first",
                );
            }
            self.proof_jobs.jobs.remove(&key);
            return json!({"ok":true,"proof_id":id,"released":true,"root_changed":false});
        }
        let cancellation = if action == "proof_cancel" {
            Some(match service.queue.cancel(handle) {
                ProofJobCancelOutcome::CancelledBeforeStart => "cancelled_before_start",
                ProofJobCancelOutcome::TooLate => "too_late",
                ProofJobCancelOutcome::UnknownHandle => "unknown",
            })
        } else {
            None
        };
        let Some(inspected) = service.queue.inspect(handle) else {
            return failure("unknown_proof", "proof no longer retained");
        };
        drop(service);
        let current = inspected.lineage.document_epoch == epoch
            && inspected.lineage.document_revision == self.font.project.document_revision();
        let mut result = json!({"ok":true,"proof_id":id,"root_changed":false,
        "captured_document_epoch":inspected.lineage.document_epoch,
        "captured_document_revision":inspected.lineage.document_revision,
        "current":current,"stale":!current,
        "status":match inspected.status {
            ProofJobStatus::Queued => "queued",
            ProofJobStatus::Running => "running",
            ProofJobStatus::Completed => "completed",
            ProofJobStatus::Failed => "failed",
            ProofJobStatus::CancelledBeforeStart => "cancelled_before_start",
        }});
        if let Some(cancellation) = cancellation {
            result["cancellation"] = json!(cancellation);
        }
        match inspected.outcome {
            Some(ProofJobOutcome::Completed(proof)) => {
                result["font_sha256"] = json!(proof.font_sha256);
                result["canonical_input_sha256"] = json!(proof.canonical_input_sha256);
                result["compiler"] = json!(proof.compiler);
                result["recipe"] = json!(proof.recipe);
                result["glyphs"] = json!(proof.glyphs);
                if include_image {
                    if proof.png.len() > MAX_PNG_BYTES {
                        return failure(
                            "proof_image_too_large",
                            "compiled PNG exceeds the transport image limit",
                        );
                    }
                    result["png_base64"] =
                        json!(base64::engine::general_purpose::STANDARD.encode(&proof.png));
                }
            }
            Some(ProofJobOutcome::Failed(error)) => {
                result["proof_error"] = json!(error);
            }
            _ => {}
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::font_model::FontModel;
    use runebender::document::project::Project;

    fn app() -> Workspace {
        Workspace::from_model(FontModel::from_project(Project::new_font(
            std::env::temp_dir().join("proof-session-never-saved.ufo"),
        )))
        .unwrap()
    }

    fn request(app: &Workspace) -> ProofStartRequest {
        ProofStartRequest {
            expected_document_epoch: app.live.as_ref().unwrap().document_epoch().into(),
            expected_document_revision: app.font.project.document_revision(),
            operation_key: "one-proof".into(),
            recipe: compiled_proof::CompiledProofRecipe {
                text: "A".into(),
                normalized_location: Vec::new(),
                right_to_left: false,
                features: Vec::new(),
                script: None,
                language: None,
            },
        }
    }

    #[test]
    fn proof_guards_reject_without_allocating_and_retry_payload_is_immutable() {
        let mut app = app();
        let valid = request(&app);
        let mut wrong = valid.clone();
        wrong.expected_document_epoch = "other-document".into();
        assert_eq!(app.start_proof(wrong)["error_code"], "stale_document");
        let mut wrong = valid.clone();
        wrong.expected_document_revision += 1;
        assert_eq!(app.start_proof(wrong)["error_code"], "stale_revision");
        let mut wrong = valid.clone();
        wrong.recipe.normalized_location = vec![0.0];
        assert_eq!(app.start_proof(wrong)["error_code"], "invalid_arguments");
        let mut unknown = serde_json::to_value(&valid).unwrap();
        unknown["recipe"]["branch"] = json!("source-only-experiment");
        let call = ToolCall {
            name: "proof_start".into(),
            arguments: unknown,
        };
        assert_eq!(app.call_live(&call)["error_code"], "invalid_arguments");
        assert!(app.proof_jobs.jobs.is_empty());
        let started = app.start_proof(valid.clone());
        assert_eq!(started["ok"], true, "{started}");
        let mut conflict = valid.clone();
        conflict.recipe.text = "B".into();
        assert_eq!(
            app.start_proof(conflict)["error_code"],
            "operation_key_conflict"
        );
        let repeated = app.start_proof(valid);
        assert_eq!(repeated["proof_id"], started["proof_id"]);
        assert_eq!(repeated["replayed"], true);
        assert_eq!(app.proof_jobs.jobs.len(), 1);
    }

    #[test]
    fn replacement_document_cannot_read_old_handles_and_reuses_the_worker() {
        let service_before = std::ptr::from_ref(service());
        let mut first = app();
        let old_request = request(&first);
        let started = first.start_proof(old_request.clone());
        let handle = started["proof_id"].as_str().unwrap();
        let mut second = app();
        let epoch = second.live.as_ref().unwrap().document_epoch().to_owned();
        assert_ne!(epoch, old_request.expected_document_epoch);
        assert_eq!(
            second.proof_action(&old_request.expected_document_epoch, handle, "status", true)["error_code"],
            "stale_document"
        );
        assert_eq!(
            second.proof_action(&epoch, handle, "status", true)["error_code"],
            "unknown_proof"
        );
        drop(first);
        assert_eq!(std::ptr::from_ref(service()), service_before);
        assert_eq!(
            second.proof_action(&epoch, handle, "status", true)["error_code"],
            "unknown_proof"
        );
    }
}
