// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Edit types for undo grouping

/// Type of edit being performed
///
/// Used to group consecutive edits of the same type into a single undo
/// action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EditType {
    /// Normal edit (creates new undo group)
    Normal,

    /// Drag operation in progress (updates current undo group)
    Drag,

    /// Drag operation completed (creates undo group if not already in
    /// one)
    DragUp,

    /// Nudge up (combines with other Up nudges)
    NudgeUp,

    /// Nudge down (combines with other Down nudges)
    NudgeDown,

    /// Nudge left (combines with other Left nudges)
    NudgeLeft,

    /// Nudge right (combines with other Right nudges)
    NudgeRight,
}
