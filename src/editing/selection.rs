// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Immutable selection set for tracking which entities are selected.
//!
//! `Selection` wraps an `Arc<BTreeSet<EntityId>>` so it can be cheaply cloned
//! for undo snapshots and shared across threads. Mutations produce a new set
//! (copy-on-write via `Arc::make_mut`). The `BTreeSet` gives deterministic
//! iteration order, which matters for multi-point operations like nudging.

use crate::model::EntityId;
use std::collections::BTreeSet;
use std::sync::Arc;

/// A set of selected entities (points, paths, guides, etc.)
///
/// Uses Arc<BTreeSet> for efficient cloning and ordered iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    inner: Arc<BTreeSet<EntityId>>,
}

impl Selection {
    /// Create a new empty selection
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BTreeSet::new()),
        }
    }

    /// Check if the selection is empty
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the number of selected entities
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Check if an entity is selected
    pub fn contains(&self, id: &EntityId) -> bool {
        self.inner.contains(id)
    }

    /// Iterate over selected entities
    pub fn iter(&self) -> impl Iterator<Item = &EntityId> {
        self.inner.iter()
    }

    /// Add an entity to the selection
    pub fn insert(&mut self, id: EntityId) {
        let mut set = (*self.inner).clone();
        set.insert(id);
        self.inner = Arc::new(set);
    }

    /// Remove an entity from the selection
    pub fn remove(&mut self, id: &EntityId) {
        let mut set = (*self.inner).clone();
        set.remove(id);
        self.inner = Arc::new(set);
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_selection_is_empty() {
        let sel = Selection::new();
        assert!(sel.is_empty());
        assert_eq!(sel.len(), 0);
    }

    #[test]
    fn insert_and_contains() {
        let mut sel = Selection::new();
        let id = EntityId::next();
        sel.insert(id);

        assert!(sel.contains(&id));
        assert!(!sel.is_empty());
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn insert_duplicate_is_noop() {
        let mut sel = Selection::new();
        let id = EntityId::next();
        sel.insert(id);
        sel.insert(id);
        assert_eq!(sel.len(), 1);
    }

    #[test]
    fn remove() {
        let mut sel = Selection::new();
        let id = EntityId::next();
        sel.insert(id);
        sel.remove(&id);

        assert!(!sel.contains(&id));
        assert!(sel.is_empty());
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let mut sel = Selection::new();
        let id = EntityId::next();
        sel.remove(&id);
        assert!(sel.is_empty());
    }

    #[test]
    fn iter_returns_all_ids() {
        let mut sel = Selection::new();
        let ids: Vec<EntityId> = (0..5).map(|_| EntityId::next()).collect();
        for &id in &ids {
            sel.insert(id);
        }

        let collected: Vec<EntityId> = sel.iter().copied().collect();
        assert_eq!(collected.len(), 5);
        for id in &ids {
            assert!(collected.contains(id));
        }
    }

    #[test]
    fn clone_is_independent() {
        let mut sel = Selection::new();
        let id1 = EntityId::next();
        let id2 = EntityId::next();
        sel.insert(id1);

        let mut clone = sel.clone();
        clone.insert(id2);

        // Original should not have id2
        assert!(!sel.contains(&id2));
        assert!(clone.contains(&id2));
    }

    #[test]
    fn equality() {
        let mut a = Selection::new();
        let mut b = Selection::new();
        let id = EntityId::next();

        a.insert(id);
        b.insert(id);
        assert_eq!(a, b);

        let id2 = EntityId::next();
        b.insert(id2);
        assert_ne!(a, b);
    }
}
