// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded paired specimens on the application's one shared compiler worker.
//!
//! A graph run retains both original captures; no inspection recaptures the document or renders
//! another image. Dropping or releasing the owner abandons running compilers for global collection.

use std::collections::BTreeMap;
use std::sync::Arc;

use runebender::font::compiler::proof::{CompileProofInput, CompiledProof, CompiledProofRecipe};
use runebender::font::compiler::proof_jobs::{
    ProofJobHandle, ProofJobLineage, ProofJobOutcome, ProofJobRequest,
};

use super::live_proofs::{ProofService, service};

const MAX_PAIRS: usize = 8;

struct Pair {
    handles: [ProofJobHandle; 2],
    lineage: ProofJobLineage,
    input_hashes: [String; 2],
    recipe: CompiledProofRecipe,
}

/// One coherent observation of the two immutable graph specimens.
pub(crate) enum NodeProofInspection {
    /// One or both jobs are still queued or compiling.
    Pending,
    /// Both exact submitted captures completed successfully.
    Completed {
        /// Artifact identities are process-wide proof handles, not node or document revisions.
        artifact_ids: [String; 2],
        /// Original and derived images, in that order, sharing the worker's retained bytes.
        proofs: [Arc<CompiledProof>; 2],
    },
    /// Neither specimen should be advertised as a successful comparison.
    Failed(String),
}

/// Per-Workspace graph ownership; it never creates its own compiler thread.
#[derive(Default)]
pub(crate) struct NodeProofJobs {
    pairs: BTreeMap<u64, Pair>,
}

impl NodeProofJobs {
    /// Queue an unchanged/derived pair with one recipe and exact captured lineage.
    pub(crate) fn submit(
        &mut self,
        run: u64,
        document_epoch: String,
        inputs: [CompileProofInput; 2],
        recipe: CompiledProofRecipe,
    ) -> Result<(), String> {
        if self.pairs.contains_key(&run) {
            return Err("graph run already owns specimen jobs".into());
        }
        if self.pairs.len() >= MAX_PAIRS {
            return Err("release an earlier graph comparison before running another".into());
        }
        recipe.validate()?;
        let [original, derived] = inputs;
        if original.document_revision() != derived.document_revision() {
            return Err("comparison inputs have different captured document revisions".into());
        }
        let lineage = ProofJobLineage {
            document_epoch,
            document_revision: original.document_revision(),
        };
        let input_hashes = [
            original.canonical_input_sha256().to_owned(),
            derived.canonical_input_sha256().to_owned(),
        ];
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        service.collect();
        let first = service
            .queue
            .submit(ProofJobRequest {
                lineage: lineage.clone(),
                input: original,
                recipe: recipe.clone(),
            })
            .map_err(|error| format!("unchanged specimen submission failed: {error:?}"))?;
        let second = match service.queue.submit(ProofJobRequest {
            lineage: lineage.clone(),
            input: derived,
            recipe: recipe.clone(),
        }) {
            Ok(handle) => handle,
            Err(error) => {
                abandon(&mut service, first);
                return Err(format!("changed specimen submission failed: {error:?}"));
            }
        };
        self.pairs.insert(
            run,
            Pair {
                handles: [first, second],
                lineage,
                input_hashes,
                recipe,
            },
        );
        Ok(())
    }

    /// Inspect only this owner's jobs; preserve the original worker-produced PNGs.
    pub(crate) fn inspect(&self, run: u64) -> Option<NodeProofInspection> {
        let pair = self.pairs.get(&run)?;
        let service = service().lock().unwrap_or_else(|error| error.into_inner());
        let mut completed = Vec::with_capacity(2);
        for (index, handle) in pair.handles.iter().enumerate() {
            let Some(job) = service.queue.inspect(*handle) else {
                return Some(NodeProofInspection::Failed(
                    "specimen job is no longer retained".into(),
                ));
            };
            if job.lineage != pair.lineage {
                return Some(NodeProofInspection::Failed(
                    "specimen lineage mismatch".into(),
                ));
            }
            match job.outcome {
                Some(ProofJobOutcome::Completed(proof)) => {
                    if proof.document_revision != pair.lineage.document_revision
                        || proof.canonical_input_sha256 != pair.input_hashes[index]
                        || proof.recipe != pair.recipe
                    {
                        return Some(NodeProofInspection::Failed(
                            "specimen capture mismatch".into(),
                        ));
                    }
                    completed.push(proof);
                }
                Some(ProofJobOutcome::Failed(message)) => {
                    return Some(NodeProofInspection::Failed(message));
                }
                Some(ProofJobOutcome::CancelledBeforeStart) => {
                    return Some(NodeProofInspection::Failed(
                        "specimen was cancelled before compilation".into(),
                    ));
                }
                None => {}
            }
        }
        let Ok(proofs) = completed.try_into() else {
            return Some(NodeProofInspection::Pending);
        };
        Some(NodeProofInspection::Completed {
            artifact_ids: pair
                .handles
                .map(|handle| format!("node-proof-{}", handle.get())),
            proofs,
        })
    }

    /// Release both handles; a running compiler is collected later without blocking the UI.
    pub(crate) fn release(&mut self, run: u64) -> bool {
        let Some(pair) = self.pairs.remove(&run) else {
            return false;
        };
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        for handle in pair.handles {
            abandon(&mut service, handle);
        }
        service.collect();
        true
    }
}

fn abandon(service: &mut ProofService, handle: ProofJobHandle) {
    service.queue.cancel(handle);
    if !service.queue.discard(handle) {
        service.abandoned.insert(handle);
    }
}

impl Drop for NodeProofJobs {
    fn drop(&mut self) {
        if self.pairs.is_empty() {
            return;
        }
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        for pair in self.pairs.values() {
            for handle in pair.handles {
                abandon(&mut service, handle);
            }
        }
        service.collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runebender::font::compiler::proof::capture;
    use runebender::font::project::{DocumentEditOperation, DocumentLayerEdit, Project};
    use runebender::font::variable::GlyphLayerAddress;
    use std::time::{Duration, Instant};

    #[test]
    fn paired_proofs_retain_exact_images_and_release_without_recompiling() {
        let mut project = Project::new_font(std::env::temp_dir().join("node-proof-unsaved.ufo"));
        project
            .add_document_glyph("A", 400.0, Some(u32::from('A')))
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(40.0, 0.0, 360.0, 700.0), false)?;
                draft.set_width(400.0)?;
                Ok(())
            })
            .unwrap();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer,
        };
        let snapshot = project.capture_document_layer(&address).unwrap();
        let transaction = project
            .begin_document_edit_transaction(
                source,
                "Node proof test",
                Vec::new(),
                vec![DocumentLayerEdit::new(
                    snapshot,
                    vec![DocumentEditOperation::SetWidth(500.0)],
                )],
            )
            .unwrap();
        let baseline = capture(&project).unwrap();
        let changed = baseline.with_staged_edit(&project, &transaction).unwrap();
        let expected = [
            baseline.canonical_input_sha256().to_owned(),
            changed.canonical_input_sha256().to_owned(),
        ];
        let recipe = CompiledProofRecipe {
            text: "AA".into(),
            normalized_location: Vec::new(),
            right_to_left: false,
            features: Vec::new(),
            script: None,
            language: None,
        };
        let mut jobs = NodeProofJobs::default();
        jobs.submit(7, "node-proof-test".into(), [baseline, changed], recipe)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let (artifacts, proofs) = loop {
            match jobs.inspect(7).unwrap() {
                NodeProofInspection::Completed {
                    artifact_ids,
                    proofs,
                } => break (artifact_ids, proofs),
                NodeProofInspection::Failed(error) => panic!("{error}"),
                NodeProofInspection::Pending => {
                    assert!(Instant::now() < deadline, "paired proofs timed out");
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        };
        assert_ne!(artifacts[0], artifacts[1]);
        assert_eq!(proofs[0].canonical_input_sha256, expected[0]);
        assert_eq!(proofs[1].canonical_input_sha256, expected[1]);
        assert_ne!(proofs[0].font_sha256, proofs[1].font_sha256);
        assert_ne!(proofs[0].png, proofs[1].png);
        let NodeProofInspection::Completed {
            proofs: observed, ..
        } = jobs.inspect(7).unwrap()
        else {
            panic!("completed pair changed state");
        };
        assert!(Arc::ptr_eq(&proofs[0], &observed[0]));
        assert!(Arc::ptr_eq(&proofs[1], &observed[1]));
        assert!(jobs.release(7));
        assert!(!jobs.release(7));
        assert!(jobs.inspect(7).is_none());
        assert_eq!(
            project.document_layer("A", &address.layer).unwrap().width(),
            400.0
        );
    }
}
