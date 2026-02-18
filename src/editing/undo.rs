// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Undo/redo system for edit operations

use std::collections::VecDeque;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum number of undo states to keep
const MAX_UNDO_HISTORY: usize = 128;

// ============================================================================
// UNDO STATE MANAGER
// ============================================================================

/// Undo/redo state manager
///
/// Stores a history of states using a deque. The current state is not
/// stored in the history - it's managed externally. The undo stack
/// contains previous states, and the redo stack contains future states.
#[derive(Debug, Clone)]
pub struct UndoState<T> {
    /// Stack of previous states (can undo to these)
    undo_stack: VecDeque<T>,

    /// Stack of future states (can redo to these)
    redo_stack: VecDeque<T>,
}

#[allow(dead_code)]
impl<T: Clone> UndoState<T> {
    /// Create a new empty undo state
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::with_capacity(MAX_UNDO_HISTORY),
            redo_stack: VecDeque::new(),
        }
    }

    /// Add a new undo group
    ///
    /// Pushes the given state onto the undo stack and clears the redo
    /// stack. If the undo stack is full, removes the oldest entry.
    pub fn add_undo_group(&mut self, state: T) {
        // Adding a new undo group clears the redo stack
        self.redo_stack.clear();

        // Add to undo stack
        self.undo_stack.push_back(state);

        // Limit history size
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
    }

    /// Update the most recent undo state without creating a new undo group
    ///
    /// This is useful for grouping rapid edits of the same type (e.g.,
    /// dragging) into a single undo operation. If there's no current undo
    /// state, this does nothing.
    pub fn update_current_undo(&mut self, state: T) {
        if let Some(last) = self.undo_stack.back_mut() {
            *last = state;
        }
    }

    /// Undo to the previous state
    ///
    /// Returns the previous state if available, moving the current state
    /// onto the redo stack. The caller is responsible for applying this
    /// state.
    ///
    /// # Arguments
    /// * `current` - The current state to push onto the redo stack
    ///
    /// # Returns
    /// The previous state, or None if there's nothing to undo
    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.undo_stack.pop_back()?;
        self.redo_stack.push_back(current);
        Some(previous)
    }

    /// Redo to the next state
    ///
    /// Returns the next state if available, moving the current state back
    /// onto the undo stack. The caller is responsible for applying this
    /// state.
    ///
    /// # Arguments
    /// * `current` - The current state to push back onto the undo stack
    ///
    /// # Returns
    /// The next state, or None if there's nothing to redo
    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.redo_stack.pop_back()?;
        self.undo_stack.push_back(current);
        Some(next)
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Clear all undo/redo history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Get the number of states in the undo stack
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get the number of states in the redo stack
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }
}

impl<T: Clone> Default for UndoState<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_undo_redo() {
        let mut undo: UndoState<i32> = UndoState::new();

        assert!(!undo.can_undo());
        assert!(!undo.can_redo());

        // Add some states
        undo.add_undo_group(1);
        undo.add_undo_group(2);

        assert!(undo.can_undo());
        assert!(!undo.can_redo());
        assert_eq!(undo.undo_depth(), 2);

        // Undo
        let prev = undo.undo(3);
        assert_eq!(prev, Some(2));
        assert_eq!(undo.undo_depth(), 1);
        assert_eq!(undo.redo_depth(), 1);

        // Redo
        let next = undo.redo(2);
        assert_eq!(next, Some(3));
        assert_eq!(undo.undo_depth(), 2);
        assert_eq!(undo.redo_depth(), 0);
    }

    #[test]
    fn test_add_clears_redo() {
        let mut undo: UndoState<i32> = UndoState::new();

        undo.add_undo_group(1);
        undo.add_undo_group(2);

        // Undo to create redo history
        undo.undo(3);
        assert_eq!(undo.redo_depth(), 1);

        // Adding new state should clear redo
        undo.add_undo_group(4);
        assert_eq!(undo.redo_depth(), 0);
    }

    #[test]
    fn test_max_history() {
        let mut undo: UndoState<i32> = UndoState::new();

        // Add more than MAX_UNDO_HISTORY states
        for i in 0..(MAX_UNDO_HISTORY + 10) {
            undo.add_undo_group(i as i32);
        }

        // Should be limited to MAX_UNDO_HISTORY
        assert_eq!(undo.undo_depth(), MAX_UNDO_HISTORY);

        // Oldest entries should be removed
        let prev = undo.undo(999);
        assert_eq!(prev, Some((MAX_UNDO_HISTORY + 9) as i32));
    }

    #[test]
    fn test_update_current_undo() {
        let mut undo: UndoState<i32> = UndoState::new();

        undo.add_undo_group(1);
        undo.update_current_undo(2);

        assert_eq!(undo.undo_depth(), 1);
        let prev = undo.undo(3);
        assert_eq!(prev, Some(2)); // Should get the updated value
    }
}
