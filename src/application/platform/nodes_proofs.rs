// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Per-node specimens on the application's one shared compiler worker.
//!
//! Each job owns an immutable capture and recipe; failure of one node does not discard siblings.
//! Dropping or releasing the owner abandons running compilers for global collection.

use std::collections::BTreeMap;
use std::sync::Arc;

use runebender::font::compiler::proof::{CompileProofInput, CompiledProof, CompiledProofRecipe};
use runebender::font::compiler::proof_jobs::{
    ProofJobHandle, ProofJobLineage, ProofJobOutcome, ProofJobRequest,
};

use super::live_proofs::{ProofService, service};

const MAX_PROOFS: usize = 32;

struct NodeProof {
    handle: ProofJobHandle,
    lineage: ProofJobLineage,
    input_hash: String,
    recipe: CompiledProofRecipe,
}

/// One coherent observation of one immutable graph specimen.
pub(crate) enum NodeProofInspection {
    /// The job is still queued or compiling.
    Pending,
    /// The exact submitted capture completed successfully.
    Completed {
        /// Process-wide artifact identity, independent of node and document revisions.
        artifact_id: String,
        /// Original worker-produced bytes, retained without rendering again.
        proof: Arc<CompiledProof>,
    },
    /// This node failed; other retained specimens remain independently available.
    Failed(String),
}

/// Per-Workspace graph ownership; it never creates its own compiler thread.
#[derive(Default)]
pub(crate) struct NodeProofJobs {
    jobs: BTreeMap<(u64, u32), NodeProof>,
}

impl NodeProofJobs {
    /// Queue one node with its own recipe and exact captured lineage.
    pub(crate) fn submit_node(
        &mut self,
        run: u64,
        node: u32,
        document_epoch: String,
        input: CompileProofInput,
        recipe: CompiledProofRecipe,
    ) -> Result<(), String> {
        if self.jobs.contains_key(&(run, node)) {
            return Err("graph node already owns a specimen job".into());
        }
        if self.jobs.len() >= MAX_PROOFS {
            return Err("release earlier graph specimens before running more proofs".into());
        }
        recipe.validate()?;
        let lineage = ProofJobLineage {
            document_epoch,
            document_revision: input.document_revision(),
        };
        let input_hash = input.canonical_input_sha256().to_owned();
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        service.collect();
        let handle = service
            .queue
            .submit(ProofJobRequest {
                lineage: lineage.clone(),
                input,
                recipe: recipe.clone(),
            })
            .map_err(|error| format!("specimen submission failed: {error:?}"))?;
        self.jobs.insert(
            (run, node),
            NodeProof {
                handle,
                lineage,
                input_hash,
                recipe,
            },
        );
        Ok(())
    }

    /// Inspect only this owner's node and preserve the original worker-produced PNG.
    pub(crate) fn inspect_node(&self, run: u64, node: u32) -> Option<NodeProofInspection> {
        let retained = self.jobs.get(&(run, node))?;
        let service = service().lock().unwrap_or_else(|error| error.into_inner());
        let Some(job) = service.queue.inspect(retained.handle) else {
            return Some(NodeProofInspection::Failed(
                "specimen job is no longer retained".into(),
            ));
        };
        if job.lineage != retained.lineage {
            return Some(NodeProofInspection::Failed(
                "specimen lineage mismatch".into(),
            ));
        }
        Some(match job.outcome {
            Some(ProofJobOutcome::Completed(proof)) => {
                if proof.document_revision != retained.lineage.document_revision
                    || proof.canonical_input_sha256 != retained.input_hash
                    || proof.recipe != retained.recipe
                {
                    NodeProofInspection::Failed("specimen capture mismatch".into())
                } else {
                    NodeProofInspection::Completed {
                        artifact_id: format!("node-proof-{}", retained.handle.get()),
                        proof,
                    }
                }
            }
            Some(ProofJobOutcome::Failed(message)) => NodeProofInspection::Failed(message),
            Some(ProofJobOutcome::CancelledBeforeStart) => {
                NodeProofInspection::Failed("specimen was cancelled before compilation".into())
            }
            None => NodeProofInspection::Pending,
        })
    }

    /// List successful retained specimens for presentation, independently of sibling failures.
    pub(crate) fn completed(&self, run: u64) -> Vec<(u32, String, Arc<CompiledProof>)> {
        self.jobs
            .keys()
            .filter(|(owner, _)| *owner == run)
            .filter_map(|(_, node)| match self.inspect_node(run, *node)? {
                NodeProofInspection::Completed { artifact_id, proof } => {
                    Some((*node, artifact_id, proof))
                }
                NodeProofInspection::Pending | NodeProofInspection::Failed(_) => None,
            })
            .collect()
    }

    /// Release one node without disturbing successful siblings.
    pub(crate) fn release_node(&mut self, run: u64, node: u32) -> bool {
        let Some(job) = self.jobs.remove(&(run, node)) else {
            return false;
        };
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        abandon(&mut service, job.handle);
        service.collect();
        true
    }

    /// Release every job in one run without blocking on a running compiler.
    pub(crate) fn release(&mut self, run: u64) -> bool {
        let nodes = self
            .jobs
            .keys()
            .filter_map(|(owner, node)| (*owner == run).then_some(*node))
            .collect::<Vec<_>>();
        let changed = !nodes.is_empty();
        for node in nodes {
            self.release_node(run, node);
        }
        changed
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
        if self.jobs.is_empty() {
            return;
        }
        let mut service = service().lock().unwrap_or_else(|error| error.into_inner());
        for job in self.jobs.values() {
            abandon(&mut service, job.handle);
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
            rendering: Default::default(),
        };
        let mut jobs = NodeProofJobs::default();
        jobs.submit_node(7, 1, "node-proof-test".into(), baseline, recipe.clone())
            .unwrap();
        jobs.submit_node(7, 2, "node-proof-test".into(), changed, recipe)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let completed = loop {
            let completed = jobs.completed(7);
            if completed.len() == 2 {
                break completed;
            }
            for node in [1, 2] {
                if let Some(NodeProofInspection::Failed(error)) = jobs.inspect_node(7, node) {
                    panic!("{error}");
                }
            }
            assert!(Instant::now() < deadline, "node proofs timed out");
            std::thread::sleep(Duration::from_millis(10));
        };
        let artifacts = [&completed[0].1, &completed[1].1];
        let proofs = [&completed[0].2, &completed[1].2];
        assert_ne!(artifacts[0], artifacts[1]);
        assert_eq!(proofs[0].canonical_input_sha256, expected[0]);
        assert_eq!(proofs[1].canonical_input_sha256, expected[1]);
        assert_ne!(proofs[0].font_sha256, proofs[1].font_sha256);
        assert_ne!(proofs[0].png, proofs[1].png);
        let observed = jobs.completed(7);
        assert!(Arc::ptr_eq(proofs[0], &observed[0].2));
        assert!(Arc::ptr_eq(proofs[1], &observed[1].2));
        assert!(jobs.release_node(7, 1));
        assert!(jobs.inspect_node(7, 1).is_none());
        assert!(matches!(
            jobs.inspect_node(7, 2),
            Some(NodeProofInspection::Completed { .. })
        ));
        assert!(jobs.release(7));
        assert!(!jobs.release(7));
        assert!(jobs.completed(7).is_empty());
        assert_eq!(
            project.document_layer("A", &address.layer).unwrap().width(),
            400.0
        );
    }
}
