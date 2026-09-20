// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical undo piles keyed by stable document addresses.

use std::collections::{BTreeMap, VecDeque};

use super::project::{
    DocumentEditOutcome, DocumentHistoryError, DocumentSourceMetadataHistoryError, Project,
};
use super::variable::GlyphLayerAddress;
use super::{CanonicalLayerSnapshot, CanonicalSourceMetadataSnapshot};

const MAX_CANONICAL_HISTORY: usize = 128;

/// Direction for replaying canonical document history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryDirection {
    /// Restore the state before the most recent edit.
    Undo,
    /// Restore the state after the most recently undone edit.
    Redo,
}

/// Result of a canonical history replay request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryReplayOutcome {
    /// The selected glyph layer had no step in the requested direction.
    Empty,
    /// The selected glyph layer replayed one step.
    Applied,
}

/// Why canonical history replay did not apply a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryReplayError<E> {
    /// The live layer no longer matches the state expected by this step.
    Stale,
    /// The document rejected the replacement without changing its state.
    Apply(E),
}

impl<E: std::fmt::Display> std::fmt::Display for HistoryReplayError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stale => formatter.write_str("history step is stale"),
            Self::Apply(error) => write!(formatter, "history replay failed: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for HistoryReplayError<E> {}

#[derive(Clone, Debug, PartialEq)]
struct CanonicalStep<S> {
    before: S,
    after: S,
}

#[derive(Clone, Debug, PartialEq)]
struct LayerHistory<S> {
    undo: VecDeque<CanonicalStep<S>>,
    redo: VecDeque<CanonicalStep<S>>,
}

impl<S> Default for LayerHistory<S> {
    fn default() -> Self {
        Self {
            undo: VecDeque::with_capacity(MAX_CANONICAL_HISTORY),
            redo: VecDeque::new(),
        }
    }
}

/// Exact before-and-after history for one guarded document transaction scope.
///
/// This stack is suitable for font-wide metadata or source-structural transactions
/// whose snapshot already contains stable identities for everything in scope.
/// Replay requires the current snapshot to match exactly and moves the stack only
/// after the caller's atomic restore operation succeeds.
#[derive(Clone, Debug, PartialEq)]
pub struct TransactionHistory<S> {
    stack: LayerHistory<S>,
}

impl<S> Default for TransactionHistory<S> {
    fn default() -> Self {
        Self {
            stack: LayerHistory::default(),
        }
    }
}

impl<S: PartialEq> TransactionHistory<S> {
    /// Record one completed transaction, clearing redo only for a real change.
    pub fn record(&mut self, before: S, after: S) -> bool {
        if before == after {
            return false;
        }
        self.stack.redo.clear();
        self.stack.undo.push_back(CanonicalStep { before, after });
        if self.stack.undo.len() > MAX_CANONICAL_HISTORY {
            self.stack.undo.pop_front();
        }
        true
    }

    /// Extend the newest transaction while retaining its original before-state.
    ///
    /// Returning to the original state removes the no-op transaction.
    pub fn coalesce(&mut self, current: &S, after: S) -> bool {
        let Some(step) = self.stack.undo.back_mut() else {
            return false;
        };
        if &step.after != current {
            return false;
        }
        step.after = after;
        if step.before == step.after {
            self.stack.undo.pop_back();
        }
        true
    }

    /// Drop the newest undo transaction after its operation reports no usable change.
    pub fn discard_last(&mut self) -> bool {
        self.stack.undo.pop_back().is_some()
    }

    /// Replay one exact transaction through an atomic caller-owned restore boundary.
    pub fn replay<E>(
        &mut self,
        current: &S,
        direction: HistoryDirection,
        apply: impl FnOnce(&S, &S) -> Result<(), E>,
    ) -> Result<HistoryReplayOutcome, HistoryReplayError<E>> {
        let step = match direction {
            HistoryDirection::Undo => self.stack.undo.back(),
            HistoryDirection::Redo => self.stack.redo.back(),
        };
        let Some(step) = step else {
            return Ok(HistoryReplayOutcome::Empty);
        };
        let (expected, replacement) = match direction {
            HistoryDirection::Undo => (&step.after, &step.before),
            HistoryDirection::Redo => (&step.before, &step.after),
        };
        if current != expected {
            return Err(HistoryReplayError::Stale);
        }
        apply(expected, replacement).map_err(HistoryReplayError::Apply)?;
        match direction {
            HistoryDirection::Undo => {
                let step = self
                    .stack
                    .undo
                    .pop_back()
                    .expect("the replayed undo transaction exists");
                self.stack.redo.push_back(step);
            }
            HistoryDirection::Redo => {
                let step = self
                    .stack
                    .redo
                    .pop_back()
                    .expect("the replayed redo transaction exists");
                self.stack.undo.push_back(step);
            }
        }
        Ok(HistoryReplayOutcome::Applied)
    }

    /// Whether a transaction can replay in `direction`.
    pub fn can_replay(&self, direction: HistoryDirection) -> bool {
        self.depth(direction) != 0
    }

    /// Number of transactions available in `direction`.
    pub fn depth(&self, direction: HistoryDirection) -> usize {
        match direction {
            HistoryDirection::Undo => self.stack.undo.len(),
            HistoryDirection::Redo => self.stack.redo.len(),
        }
    }

    /// Forget every transaction, such as after replacing the open document.
    pub fn clear(&mut self) {
        self.stack.undo.clear();
        self.stack.redo.clear();
    }
}

/// Exact before-and-after history for canonical glyph layers.
///
/// Each stack is addressed by a stable source and layer identity plus the current
/// glyph name. A rename moves the complete stack to the glyph's new name. Ordinary
/// edits retain one layer snapshot per side of a step instead of cloning the whole
/// document.
///
/// Replay is conflict safe: the caller supplies the live snapshot, which must equal
/// the side of the step being replaced. The stack moves only after `apply` succeeds,
/// so stale and rejected operations leave both document and history unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalHistory<S> {
    stacks: BTreeMap<GlyphLayerAddress, LayerHistory<S>>,
}

impl<S> Default for CanonicalHistory<S> {
    fn default() -> Self {
        Self {
            stacks: BTreeMap::new(),
        }
    }
}

impl<S: PartialEq> CanonicalHistory<S> {
    /// Record one completed canonical edit.
    ///
    /// A no-op records nothing and retains redo history. A real edit clears redo
    /// only for this glyph layer.
    pub fn record(&mut self, address: GlyphLayerAddress, before: S, after: S) -> bool {
        if before == after {
            return false;
        }
        let stack = self.stacks.entry(address).or_default();
        stack.redo.clear();
        stack.undo.push_back(CanonicalStep { before, after });
        if stack.undo.len() > MAX_CANONICAL_HISTORY {
            stack.undo.pop_front();
        }
        true
    }

    /// Extend the latest step with the next state in the same gesture.
    ///
    /// `current` must equal the latest recorded after-state. The original before-state
    /// is retained, so a drag remains one undo step. Returning to the original state
    /// removes the no-op step.
    pub fn coalesce(&mut self, address: &GlyphLayerAddress, current: &S, after: S) -> bool {
        let Some(stack) = self.stacks.get_mut(address) else {
            return false;
        };
        let Some(step) = stack.undo.back_mut() else {
            return false;
        };
        if &step.after != current {
            return false;
        }
        step.after = after;
        if step.before == step.after {
            stack.undo.pop_back();
        }
        true
    }

    /// Drop the most recently recorded undo step for one glyph layer.
    ///
    /// This is used when a gesture opened a history group but its operation later
    /// proved invalid or unchanged.
    pub fn discard_last(&mut self, address: &GlyphLayerAddress) -> bool {
        self.stacks
            .get_mut(address)
            .is_some_and(|stack| stack.undo.pop_back().is_some())
    }

    /// Atomically replay one step through the document's canonical restore operation.
    ///
    /// `current` is compared before `apply` runs. The closure receives the expected
    /// live state and replacement so the document boundary can repeat the comparison
    /// in the same transaction that installs the replacement.
    pub fn replay<E>(
        &mut self,
        address: &GlyphLayerAddress,
        current: &S,
        direction: HistoryDirection,
        apply: impl FnOnce(&S, &S) -> Result<(), E>,
    ) -> Result<HistoryReplayOutcome, HistoryReplayError<E>> {
        let Some(stack) = self.stacks.get_mut(address) else {
            return Ok(HistoryReplayOutcome::Empty);
        };
        let step = match direction {
            HistoryDirection::Undo => stack.undo.back(),
            HistoryDirection::Redo => stack.redo.back(),
        };
        let Some(step) = step else {
            return Ok(HistoryReplayOutcome::Empty);
        };
        let (expected, replacement) = match direction {
            HistoryDirection::Undo => (&step.after, &step.before),
            HistoryDirection::Redo => (&step.before, &step.after),
        };
        if current != expected {
            return Err(HistoryReplayError::Stale);
        }
        apply(expected, replacement).map_err(HistoryReplayError::Apply)?;
        match direction {
            HistoryDirection::Undo => {
                let step = stack
                    .undo
                    .pop_back()
                    .expect("the replayed undo step exists");
                stack.redo.push_back(step);
            }
            HistoryDirection::Redo => {
                let step = stack
                    .redo
                    .pop_back()
                    .expect("the replayed redo step exists");
                stack.undo.push_back(step);
            }
        }
        Ok(HistoryReplayOutcome::Applied)
    }

    /// Whether a glyph layer has a step in `direction`.
    pub fn can_replay(&self, address: &GlyphLayerAddress, direction: HistoryDirection) -> bool {
        self.depth(address, direction) != 0
    }

    /// Number of steps available for one glyph layer in `direction`.
    pub fn depth(&self, address: &GlyphLayerAddress, direction: HistoryDirection) -> usize {
        self.stacks.get(address).map_or(0, |stack| match direction {
            HistoryDirection::Undo => stack.undo.len(),
            HistoryDirection::Redo => stack.redo.len(),
        })
    }

    /// Move every layer history when a glyph is renamed.
    ///
    /// Returns false without changing any stack if the new name already has history.
    pub fn rename_glyph(&mut self, old: &str, new: &str) -> bool {
        if old == new {
            return false;
        }
        let old_addresses = self
            .stacks
            .keys()
            .filter(|address| address.glyph == old)
            .cloned()
            .collect::<Vec<_>>();
        if self.stacks.keys().any(|address| address.glyph == new) {
            return false;
        }
        for old_address in old_addresses {
            let stack = self
                .stacks
                .remove(&old_address)
                .expect("the collected history address exists");
            self.stacks.insert(
                GlyphLayerAddress {
                    glyph: new.to_owned(),
                    layer: old_address.layer,
                },
                stack,
            );
        }
        true
    }

    /// Forget every layer history for one glyph after permanent removal.
    pub fn clear_glyph(&mut self, name: &str) {
        self.stacks.retain(|address, _| address.glyph != name);
    }

    /// Forget one layer's history after permanent removal.
    pub fn clear_layer(&mut self, address: &GlyphLayerAddress) {
        self.stacks.remove(address);
    }

    /// Forget all canonical history, such as after replacing the open document.
    pub fn clear(&mut self) {
        self.stacks.clear();
    }
}

impl<S: Clone + PartialEq> CanonicalHistory<S> {
    fn rename_glyph_with(
        &mut self,
        old: &str,
        new: &str,
        mut rebind: impl FnMut(&GlyphLayerAddress, &GlyphLayerAddress, S) -> Option<S>,
    ) -> bool {
        if old == new || self.stacks.keys().any(|address| address.glyph == new) {
            return false;
        }
        let old_addresses = self
            .stacks
            .keys()
            .filter(|address| address.glyph == old)
            .cloned()
            .collect::<Vec<_>>();
        let mut rebound = Vec::with_capacity(old_addresses.len());
        for old_address in &old_addresses {
            let new_address = GlyphLayerAddress {
                glyph: new.to_owned(),
                layer: old_address.layer.clone(),
            };
            let mut stack = self
                .stacks
                .get(old_address)
                .expect("the collected history address exists")
                .clone();
            for step in stack.undo.iter_mut().chain(&mut stack.redo) {
                step.before = match rebind(old_address, &new_address, step.before.clone()) {
                    Some(snapshot) => snapshot,
                    None => return false,
                };
                step.after = match rebind(old_address, &new_address, step.after.clone()) {
                    Some(snapshot) => snapshot,
                    None => return false,
                };
            }
            rebound.push((new_address, stack));
        }
        for old_address in old_addresses {
            self.stacks.remove(&old_address);
        }
        self.stacks.extend(rebound);
        true
    }
}

/// Document-owned history over complete canonical layer snapshots.
///
/// Callers capture the before-state, commit one or more direct document edits and
/// then record the completed state. Undo and redo use Project's guarded canonical
/// restore boundary, so history never stores or materializes a UFO glyph.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentHistory {
    layers: CanonicalHistory<CanonicalLayerSnapshot>,
}

impl DocumentHistory {
    /// Capture one canonical layer before a document edit.
    pub fn capture(
        project: &Project,
        address: &GlyphLayerAddress,
    ) -> Result<CanonicalLayerSnapshot, DocumentHistoryError> {
        project
            .capture_document_layer(address)
            .ok_or_else(|| DocumentHistoryError::MissingLayer(address.clone()))
    }

    /// Record the live state after an edit, paired with its captured before-state.
    ///
    /// A failed or no-op document edit produces equal snapshots, records nothing and
    /// does not invalidate redo.
    pub fn record_completed(
        &mut self,
        project: &Project,
        address: &GlyphLayerAddress,
        before: CanonicalLayerSnapshot,
    ) -> Result<bool, DocumentHistoryError> {
        if before.address() != address {
            return Err(DocumentHistoryError::AddressMismatch(address.clone()));
        }
        let after = Self::capture(project, address)?;
        Ok(self.layers.record(address.clone(), before, after))
    }

    /// Extend the latest history step with the live result of the next drag edit.
    ///
    /// `previous` is the state captured immediately before that edit. Returning the
    /// layer to the gesture origin removes the resulting no-op step.
    pub fn coalesce_completed(
        &mut self,
        project: &Project,
        address: &GlyphLayerAddress,
        previous: &CanonicalLayerSnapshot,
    ) -> Result<bool, DocumentHistoryError> {
        if previous.address() != address {
            return Err(DocumentHistoryError::AddressMismatch(address.clone()));
        }
        let after = Self::capture(project, address)?;
        Ok(self.layers.coalesce(address, previous, after))
    }

    /// Drop the newest undo step after an operation reports no usable change.
    pub fn discard_last(&mut self, address: &GlyphLayerAddress) -> bool {
        self.layers.discard_last(address)
    }

    /// Undo or redo one canonical layer step through guarded Project restoration.
    pub fn replay(
        &mut self,
        project: &mut Project,
        address: &GlyphLayerAddress,
        direction: HistoryDirection,
    ) -> Result<HistoryReplayOutcome, HistoryReplayError<DocumentHistoryError>> {
        let current = Self::capture(project, address).map_err(HistoryReplayError::Apply)?;
        self.layers
            .replay(address, &current, direction, |expected, replacement| {
                let outcome = project.restore_document_layer_if_current(
                    address,
                    expected,
                    replacement.clone(),
                )?;
                match outcome {
                    DocumentEditOutcome::Changed { .. } => Ok(()),
                    DocumentEditOutcome::Unchanged { .. } => {
                        debug_assert!(false, "a recorded history step must change its layer");
                        Err(DocumentHistoryError::StaleLayer(address.clone()))
                    }
                }
            })
    }

    /// Whether one canonical layer can replay in `direction`.
    pub fn can_replay(&self, address: &GlyphLayerAddress, direction: HistoryDirection) -> bool {
        self.layers.can_replay(address, direction)
    }

    /// Number of canonical layer steps available in `direction`.
    pub fn depth(&self, address: &GlyphLayerAddress, direction: HistoryDirection) -> usize {
        self.layers.depth(address, direction)
    }

    /// Move every canonical layer stack when a glyph is renamed.
    pub fn rename_glyph(&mut self, old: &str, new: &str) -> bool {
        self.layers
            .rename_glyph_with(old, new, |old_address, new_address, mut snapshot| {
                snapshot
                    .rebind_glyph(old_address, new_address)
                    .then_some(snapshot)
            })
    }

    /// Forget every canonical layer stack after a glyph is permanently removed.
    pub fn clear_glyph(&mut self, name: &str) {
        self.layers.clear_glyph(name);
    }

    /// Forget one canonical layer stack after that layer is permanently removed.
    pub fn clear_layer(&mut self, address: &GlyphLayerAddress) {
        self.layers.clear_layer(address);
    }

    /// Forget all canonical history after replacing the open document.
    pub fn clear(&mut self) {
        self.layers.clear();
    }
}

/// Document-owned history over the complete canonical source-metadata set.
///
/// One transaction may change metadata in several sources. Snapshots are keyed by
/// stable source identity rather than display order, and Project restores the whole
/// set atomically so replay cannot partially update a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceMetadataHistory {
    transactions: TransactionHistory<CanonicalSourceMetadataSnapshot>,
}

impl SourceMetadataHistory {
    /// Capture canonical metadata for every current source before a transaction.
    pub fn capture(project: &Project) -> CanonicalSourceMetadataSnapshot {
        project.capture_document_source_metadata()
    }

    /// Record the complete live source-metadata set after a transaction.
    ///
    /// An unchanged transaction records nothing and retains redo.
    pub fn record_completed(
        &mut self,
        project: &Project,
        before: CanonicalSourceMetadataSnapshot,
    ) -> bool {
        let after = Self::capture(project);
        self.transactions.record(before, after)
    }

    /// Extend the newest transaction with the current complete metadata set.
    ///
    /// `previous` must equal the after-state of the newest transaction. Returning
    /// every source to the transaction origin removes the resulting no-op step.
    pub fn coalesce_completed(
        &mut self,
        project: &Project,
        previous: &CanonicalSourceMetadataSnapshot,
    ) -> bool {
        self.transactions.coalesce(previous, Self::capture(project))
    }

    /// Drop the newest undo transaction after an operation reports no usable change.
    pub fn discard_last(&mut self) -> bool {
        self.transactions.discard_last()
    }

    /// Undo or redo one whole-source metadata transaction through guarded restoration.
    pub fn replay(
        &mut self,
        project: &mut Project,
        direction: HistoryDirection,
    ) -> Result<HistoryReplayOutcome, HistoryReplayError<DocumentSourceMetadataHistoryError>> {
        let current = Self::capture(project);
        self.transactions
            .replay(&current, direction, |expected, replacement| {
                let outcome = project
                    .restore_document_source_metadata_if_current(expected, replacement.clone())?;
                match outcome {
                    DocumentEditOutcome::Changed { .. } => Ok(()),
                    DocumentEditOutcome::Unchanged { .. } => {
                        debug_assert!(false, "a recorded history transaction must change metadata");
                        Err(DocumentSourceMetadataHistoryError::Stale)
                    }
                }
            })
    }

    /// Whether a source-metadata transaction can replay in `direction`.
    pub fn can_replay(&self, direction: HistoryDirection) -> bool {
        self.transactions.can_replay(direction)
    }

    /// Number of source-metadata transactions available in `direction`.
    pub fn depth(&self, direction: HistoryDirection) -> usize {
        self.transactions.depth(direction)
    }

    /// Forget all source-metadata history after replacing the open document.
    pub fn clear(&mut self) {
        self.transactions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct BoundState {
        address: GlyphLayerAddress,
        value: i32,
    }

    fn bound_address(glyph: &str) -> GlyphLayerAddress {
        GlyphLayerAddress {
            glyph: glyph.into(),
            layer: super::super::variable::LayerId {
                source: super::super::variable::SourceId(7),
                name: "public.default".into(),
            },
        }
    }

    #[test]
    fn canonical_rename_rebinds_address_bound_undo_and_redo_atomically() {
        let old = bound_address("a");
        let new = bound_address("a.alt");
        let before = BoundState {
            address: old.clone(),
            value: 1,
        };
        let after = BoundState {
            address: old.clone(),
            value: 2,
        };
        let mut history = CanonicalHistory::default();
        assert!(history.record(old.clone(), before, after.clone()));
        let mut live = after;
        assert_eq!(
            history.replay(&old, &live.clone(), HistoryDirection::Undo, |_, state| {
                live = state.clone();
                Ok::<_, ()>(())
            }),
            Ok(HistoryReplayOutcome::Applied)
        );

        assert!(
            history.rename_glyph_with("a", "a.alt", |old, new, mut state| {
                if &state.address != old {
                    return None;
                }
                state.address = new.clone();
                Some(state)
            })
        );
        live.address = new.clone();
        assert_eq!(history.depth(&new, HistoryDirection::Redo), 1);
        assert_eq!(
            history.replay(&new, &live.clone(), HistoryDirection::Redo, |_, state| {
                live = state.clone();
                Ok::<_, ()>(())
            }),
            Ok(HistoryReplayOutcome::Applied)
        );
        assert_eq!(live.address, new);
        assert_eq!(live.value, 2);
    }

    #[test]
    fn guarded_transaction_history_rejects_stale_and_failed_replay() {
        let mut history = TransactionHistory::default();
        assert!(history.record(1, 2));
        assert_eq!(
            history.replay(&3, HistoryDirection::Undo, |_, _| Ok::<_, ()>(())),
            Err(HistoryReplayError::Stale)
        );
        assert_eq!(history.depth(HistoryDirection::Undo), 1);
        assert_eq!(
            history.replay(&2, HistoryDirection::Undo, |_, _| Err("rejected")),
            Err(HistoryReplayError::Apply("rejected"))
        );
        assert_eq!(history.depth(HistoryDirection::Undo), 1);
        assert_eq!(history.depth(HistoryDirection::Redo), 0);

        let mut live = 2;
        let current = live;
        assert_eq!(
            history.replay(&current, HistoryDirection::Undo, |_, replacement| {
                live = *replacement;
                Ok::<_, ()>(())
            }),
            Ok(HistoryReplayOutcome::Applied)
        );
        assert_eq!(live, 1);
        assert_eq!(history.depth(HistoryDirection::Redo), 1);
        assert!(!history.record(live, live));
        assert_eq!(history.depth(HistoryDirection::Redo), 1);
        assert!(history.record(live, 4));
        assert_eq!(history.depth(HistoryDirection::Redo), 0);
    }
}
