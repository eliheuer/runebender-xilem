// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Guarded Project commit for complete canonical proposal batches.

use super::*;
use crate::font::CanonicalSourceStructureSnapshot;

/// One fully staged proposal layer guarded by the complete canonical source structure.
#[derive(Clone, Debug)]
pub(in crate::font) struct CanonicalProposalTransaction {
    base: CanonicalSourceStructureSnapshot,
    replacement: CanonicalSourceStructureSnapshot,
    source: SourceId,
    affected: Vec<GlyphLayerAddress>,
}

impl Project {
    /// Stage every proposal glyph without changing the project or its histories.
    pub(in crate::font) fn begin_proposal_transaction(
        &self,
        source: SourceId,
        foreground: &LayerId,
        target: &LayerId,
        staged: Vec<(String, super::super::LayerEditDraft)>,
    ) -> Result<CanonicalProposalTransaction, String> {
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let affected = replacement.stage_proposal_layers(source, foreground, target, staged)?;
        Ok(CanonicalProposalTransaction {
            base,
            replacement,
            source,
            affected,
        })
    }

    /// Commit a complete proposal layer if its captured source structure is still current.
    pub(in crate::font) fn commit_proposal_transaction(
        &mut self,
        transaction: CanonicalProposalTransaction,
    ) -> Result<Vec<GlyphLayerAddress>, String> {
        let changed = self
            .variable
            .restore_source_structure_if_current(&transaction.base, transaction.replacement)
            .map_err(|_| "canonical source structure changed after proposal planning")?;
        if !changed {
            return Err("proposal transaction made no change".into());
        }
        let index = self
            .source_index(transaction.source)
            .expect("validated proposal source remains present");
        self.sources[index].dirty = true;
        self.compute_compat();
        Ok(transaction.affected)
    }
}
