// Copyright 2024 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

//! Gap buffer implementation for efficient text editing.
//!
//! A gap buffer maintains a contiguous array with a "gap" of unused space
//! at the cursor position. This allows O(1) insertion and deletion at the
//! cursor, with O(n) worst-case for moving the gap.

use super::data::Sort;

/// A gap buffer for sorts, optimized for text editing operations.
///
/// The buffer maintains a gap of unused space at the cursor position.
/// Layout: [active elements] [gap] [active elements]
///         ^                 ^     ^
///         0            gap_start  gap_end
///
/// Invariants:
/// - 0 <= gap_start <= gap_end <= buffer.len()
/// - cursor is the logical position (0..=active_element_count)
#[derive(Debug, Clone)]
pub struct SortBuffer {
    /// The underlying storage (includes gap)
    buffer: Vec<Sort>,
    /// Start of the gap (inclusive)
    gap_start: usize,
    /// End of the gap (exclusive)
    gap_end: usize,
    /// Logical cursor position (where insertions occur)
    cursor: usize,
}

impl SortBuffer {
    /// Initial gap size when creating a new buffer
    const INITIAL_GAP_SIZE: usize = 16;
    /// Minimum gap size to maintain after growth
    const MIN_GAP_SIZE: usize = 16;

    /// Create a new empty sort buffer.
    pub fn new() -> Self {
        let mut buffer = Vec::with_capacity(Self::INITIAL_GAP_SIZE);
        buffer.resize_with(Self::INITIAL_GAP_SIZE, Sort::default);

        Self {
            buffer,
            gap_start: 0,
            gap_end: Self::INITIAL_GAP_SIZE,
            cursor: 0,
        }
    }

    /// Create a new buffer with the given initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let total_capacity = capacity + Self::INITIAL_GAP_SIZE;
        let mut buffer = Vec::with_capacity(total_capacity);
        buffer.resize_with(total_capacity, Sort::default);

        Self {
            buffer,
            gap_start: 0,
            gap_end: total_capacity,
            cursor: 0,
        }
    }

    /// Get the number of active sorts (excluding the gap).
    pub fn len(&self) -> usize {
        self.buffer.len() - self.gap_size()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the current cursor position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Get the size of the gap.
    fn gap_size(&self) -> usize {
        self.gap_end - self.gap_start
    }

    /// Move the gap to the specified position.
    ///
    /// This is the core operation that makes gap buffers efficient.
    /// Moving the gap has O(k) complexity where k is the distance moved.
    fn move_gap_to(&mut self, position: usize) {
        if position == self.gap_start {
            return; // Gap already at position
        }

        if position < self.gap_start {
            // Move gap left: copy elements from before gap to after gap
            let move_count = self.gap_start - position;
            let src_start = position;
            let dst_start = self.gap_end - move_count;

            // Copy elements: [pos..gap_start] -> [gap_end-count..gap_end]
            for i in (0..move_count).rev() {
                self.buffer[dst_start + i] = self.buffer[src_start + i].clone();
            }

            self.gap_end -= move_count;
            self.gap_start = position;
        } else {
            // Move gap right: copy elements from after gap to before gap
            let move_count = position - self.gap_start;
            let src_start = self.gap_end;
            let dst_start = self.gap_start;

            // Copy elements: [gap_end..gap_end+count] -> [gap_start..pos]
            for i in 0..move_count {
                self.buffer[dst_start + i] = self.buffer[src_start + i].clone();
            }

            self.gap_start += move_count;
            self.gap_end += move_count;
        }
    }

    /// Grow the gap when it becomes too small.
    ///
    /// Doubles the buffer capacity and moves elements after the gap to the end.
    fn grow_gap(&mut self) {
        let old_len = self.buffer.len();
        let new_capacity = (old_len * 2).max(Self::MIN_GAP_SIZE);
        let additional = new_capacity - old_len;

        // Reserve additional space
        self.buffer.reserve(additional);

        // Move elements after gap to the end
        let elements_after_gap = old_len - self.gap_end;
        if elements_after_gap > 0 {
            // Extend buffer with default elements
            self.buffer
                .resize_with(new_capacity, Sort::default);

            // Copy elements from after old gap to end of new buffer
            let new_gap_end = new_capacity - elements_after_gap;
            for i in 0..elements_after_gap {
                self.buffer[new_gap_end + i] = self.buffer[self.gap_end + i].clone();
            }

            self.gap_end = new_gap_end;
        } else {
            // No elements after gap, just extend
            self.buffer
                .resize_with(new_capacity, Sort::default);
            self.gap_end = new_capacity;
        }
    }

    /// Insert a sort at the current cursor position.
    ///
    /// Complexity: O(1) if gap is at cursor, O(n) if gap needs to move.
    pub fn insert(&mut self, sort: Sort) {
        // Grow gap if needed
        if self.gap_size() == 0 {
            self.grow_gap();
        }

        // Move gap to cursor
        self.move_gap_to(self.cursor);

        // Insert at gap start
        self.buffer[self.gap_start] = sort;
        self.gap_start += 1;

        // Move cursor forward
        self.cursor += 1;
    }

    /// Delete the sort before the cursor (like backspace).
    ///
    /// Returns the deleted sort if there was one.
    /// Complexity: O(1) if gap is at cursor, O(n) if gap needs to move.
    pub fn delete(&mut self) -> Option<Sort> {
        if self.cursor == 0 {
            return None; // Nothing to delete
        }

        // Move gap to cursor
        self.move_gap_to(self.cursor);

        // Expand gap to include element before cursor
        self.gap_start -= 1;
        self.cursor -= 1;

        Some(self.buffer[self.gap_start].clone())
    }

    /// Delete the sort at the cursor (like delete key).
    ///
    /// Returns the deleted sort if there was one.
    pub fn delete_forward(&mut self) -> Option<Sort> {
        if self.cursor >= self.len() {
            return None; // Nothing to delete
        }

        // Move gap to cursor
        self.move_gap_to(self.cursor);

        // Expand gap to include element at cursor
        let deleted = self.buffer[self.gap_end].clone();
        self.gap_end += 1;

        Some(deleted)
    }

    /// Move cursor left by one position.
    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right by one position.
    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.len() {
            self.cursor += 1;
        }
    }

    /// Set cursor to a specific position.
    ///
    /// Position is clamped to valid range [0, len()].
    pub fn set_cursor(&mut self, position: usize) {
        self.cursor = position.min(self.len());
    }

    /// Get a sort at the given logical index.
    ///
    /// Returns None if index is out of bounds.
    pub fn get(&self, index: usize) -> Option<&Sort> {
        if index >= self.len() {
            return None;
        }

        // Map logical index to physical index
        let physical_index = if index < self.gap_start {
            index
        } else {
            index + self.gap_size()
        };

        self.buffer.get(physical_index)
    }

    /// Get a mutable reference to a sort at the given logical index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Sort> {
        if index >= self.len() {
            return None;
        }

        // Map logical index to physical index
        let physical_index = if index < self.gap_start {
            index
        } else {
            index + self.gap_size()
        };

        self.buffer.get_mut(physical_index)
    }

    /// Set the active state for all sorts.
    pub fn set_all_inactive(&mut self) {
        for i in 0..self.len() {
            if let Some(sort) = self.get_mut(i) {
                sort.is_active = false;
            }
        }
    }

    /// Set a specific sort as active, deactivating all others.
    pub fn set_active_sort(&mut self, index: usize) {
        // First deactivate all
        self.set_all_inactive();

        // Then activate the target
        if let Some(sort) = self.get_mut(index) {
            sort.is_active = true;
        }
    }

    /// Find the index of the active sort, if any.
    pub fn find_active_sort(&self) -> Option<usize> {
        for i in 0..self.len() {
            if let Some(sort) = self.get(i)
                && sort.is_active {
                    return Some(i);
                }
        }
        None
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.gap_start = 0;
        self.gap_end = self.buffer.len();
        self.cursor = 0;
    }

    /// Iterate over all active sorts (skipping the gap).
    pub fn iter(&self) -> SortBufferIter<'_> {
        SortBufferIter {
            buffer: self,
            index: 0,
        }
    }
}

impl Default for SortBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over sorts in a buffer.
pub struct SortBufferIter<'a> {
    buffer: &'a SortBuffer,
    index: usize,
}

impl<'a> Iterator for SortBufferIter<'a> {
    type Item = &'a Sort;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.buffer.len() {
            return None;
        }

        let sort = self.buffer.get(self.index);
        self.index += 1;
        sort
    }
}

impl<'a> ExactSizeIterator for SortBufferIter<'a> {
    fn len(&self) -> usize {
        self.buffer.len() - self.index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_buffer() {
        let buffer = SortBuffer::new();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(buffer.cursor(), 0);
    }

    #[test]
    fn test_insert_single() {
        let mut buffer = SortBuffer::new();
        let sort = Sort::new_glyph("a".to_string(), Some('a'), 500.0, false);

        buffer.insert(sort);

        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer.cursor(), 1);
        assert_eq!(buffer.get(0).unwrap().glyph_name(), Some("a"));
    }

    #[test]
    fn test_insert_multiple() {
        let mut buffer = SortBuffer::new();

        for c in "Hello".chars() {
            let sort = Sort::new_glyph(c.to_string(), Some(c), 500.0, false);
            buffer.insert(sort);
        }

        assert_eq!(buffer.len(), 5);
        assert_eq!(buffer.cursor(), 5);

        let text: String = buffer
            .iter()
            .filter_map(|s| s.glyph_name())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(text, "Hello");
    }

    #[test]
    fn test_delete_backspace() {
        let mut buffer = SortBuffer::new();

        for c in "abc".chars() {
            buffer.insert(Sort::new_glyph(c.to_string(), Some(c), 500.0, false));
        }

        assert_eq!(buffer.len(), 3);

        let deleted = buffer.delete();
        assert_eq!(deleted.unwrap().glyph_name(), Some("c"));
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.cursor(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut buffer = SortBuffer::new();

        for c in "abc".chars() {
            buffer.insert(Sort::new_glyph(c.to_string(), Some(c), 500.0, false));
        }

        assert_eq!(buffer.cursor(), 3);

        buffer.move_cursor_left();
        assert_eq!(buffer.cursor(), 2);

        buffer.move_cursor_left();
        buffer.move_cursor_left();
        assert_eq!(buffer.cursor(), 0);

        buffer.move_cursor_left(); // Should not go below 0
        assert_eq!(buffer.cursor(), 0);

        buffer.move_cursor_right();
        assert_eq!(buffer.cursor(), 1);
    }

    #[test]
    fn test_insert_at_middle() {
        let mut buffer = SortBuffer::new();

        // Insert "ac"
        buffer.insert(Sort::new_glyph("a".to_string(), Some('a'), 500.0, false));
        buffer.insert(Sort::new_glyph("c".to_string(), Some('c'), 500.0, false));

        // Move cursor back and insert "b"
        buffer.move_cursor_left();
        buffer.insert(Sort::new_glyph("b".to_string(), Some('b'), 500.0, false));

        let text: String = buffer
            .iter()
            .filter_map(|s| s.glyph_name())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(text, "abc");
    }

    #[test]
    fn test_gap_growth() {
        let mut buffer = SortBuffer::new();

        // Insert many elements to trigger gap growth
        for i in 0..100 {
            let sort = Sort::new_glyph(i.to_string(), None, 500.0, false);
            buffer.insert(sort);
        }

        assert_eq!(buffer.len(), 100);
        assert_eq!(buffer.cursor(), 100);
    }

    #[test]
    fn test_active_sort() {
        let mut buffer = SortBuffer::new();

        buffer.insert(Sort::new_glyph("a".to_string(), Some('a'), 500.0, false));
        buffer.insert(Sort::new_glyph("b".to_string(), Some('b'), 500.0, false));
        buffer.insert(Sort::new_glyph("c".to_string(), Some('c'), 500.0, false));

        assert_eq!(buffer.find_active_sort(), None);

        buffer.set_active_sort(1);
        assert_eq!(buffer.find_active_sort(), Some(1));
        assert!(buffer.get(1).unwrap().is_active);
        assert!(!buffer.get(0).unwrap().is_active);
        assert!(!buffer.get(2).unwrap().is_active);
    }

    #[test]
    fn test_iterator() {
        let mut buffer = SortBuffer::new();

        for c in "test".chars() {
            buffer.insert(Sort::new_glyph(c.to_string(), Some(c), 500.0, false));
        }

        let count = buffer.iter().count();
        assert_eq!(count, 4);

        let text: String = buffer
            .iter()
            .filter_map(|s| s.glyph_name())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(text, "test");
    }

    #[test]
    fn test_delete_forward() {
        let mut buffer = SortBuffer::new();

        for c in "abc".chars() {
            buffer.insert(Sort::new_glyph(c.to_string(), Some(c), 500.0, false));
        }

        // Move cursor to position 1 (before 'b')
        buffer.set_cursor(1);
        assert_eq!(buffer.cursor(), 1);

        // Delete forward should remove 'b'
        let deleted = buffer.delete_forward();
        assert_eq!(deleted.unwrap().glyph_name(), Some("b"));
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.cursor(), 1);

        let text: String = buffer
            .iter()
            .filter_map(|s| s.glyph_name())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(text, "ac");
    }
}
