// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Edit types for undo grouping.

/// The type of edit being performed.
///
/// Consecutive edits of the same type are grouped into a single undo
/// action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditType {
    /// A normal edit. Creates a new undo group.
    Normal,

    /// A drag in progress. Updates the current undo group.
    Drag,

    /// A completed drag. Creates an undo group when not already in
    /// one.
    DragUp,

    /// A nudge up. Combines with other up nudges.
    NudgeUp,

    /// A nudge down. Combines with other down nudges.
    NudgeDown,

    /// A nudge left. Combines with other left nudges.
    NudgeLeft,

    /// A nudge right. Combines with other right nudges.
    NudgeRight,

    /// A transform: flip, rotate, scale, or the like.
    Transform,
}
