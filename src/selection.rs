// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Selection management for editing

use crate::entity_id::EntityId;
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
