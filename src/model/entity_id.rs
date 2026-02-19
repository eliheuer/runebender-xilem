// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Unique identifiers for paths, points, guides, and components.
//!
//! Each `EntityId` is a monotonically increasing `u64` generated from a global
//! atomic counter. IDs are used as keys in `Selection` sets and for matching
//! click targets to path elements during hit testing. They are never reused
//! within a session, so deleted points leave no dangling references.

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
}

impl Default for EntityId {
    fn default() -> Self {
        Self::next()
    }
}
