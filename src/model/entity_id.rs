// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Unique identifiers for paths, points, guides, and components

use std::sync::atomic::{AtomicU64, Ordering};

/// A unique identifier for an entity (point, path, guide, component)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u64);

static ENTITY_COUNTER: AtomicU64 = AtomicU64::new(1);

impl EntityId {
    /// Create a new unique entity ID
    pub fn next() -> Self {
        Self(ENTITY_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the raw ID value (useful for debugging)
    #[allow(dead_code)]
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::next()
    }
}
