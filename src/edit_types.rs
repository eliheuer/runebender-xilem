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

#[allow(dead_code)]
impl EditType {
    /// Check if this edit type should create a new undo group when
    /// following the given previous edit type
    pub fn should_create_new_undo_group(
        &self,
        prev: Option<EditType>,
    ) -> bool {
        match (prev, self) {
            // No previous edit - create new group
            (None, _) => true,

            // Drag continues in same group
            (Some(EditType::Drag), EditType::Drag) => false,

            // DragUp creates group if not already dragging
            (Some(EditType::Drag), EditType::DragUp) => false,
            (_, EditType::DragUp) => true,

            // Same nudge direction continues in same group
            (Some(prev), current)
                if prev == *current && prev.is_nudge() =>
            {
                false
            }

            // Different edit types create new groups
            _ => true,
        }
    }

    /// Check if this is a nudge operation
    pub fn is_nudge(&self) -> bool {
        matches!(
            self,
            EditType::NudgeUp
                | EditType::NudgeDown
                | EditType::NudgeLeft
                | EditType::NudgeRight
        )
    }
}

