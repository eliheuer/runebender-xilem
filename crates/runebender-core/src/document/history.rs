// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The one undo pile.
//!
//! Every shell used to keep its own stack of glyph snapshots, with its
//! own idea of what counts as one step. This module keeps the pile in
//! core, one stack per glyph name, so the editor, the command line,
//! and a model proposal all push and pop the same way. A shell calls
//! [`EditHistory::record`] before it changes a glyph and
//! [`EditHistory::undo`] when the user asks; it never holds a snapshot
//! itself.
//!
//! Stacks are keyed by glyph name, so history survives switching
//! glyphs and the grid, and a font-wide operation (a proposed master)
//! leaves one step per glyph, undone one glyph at a time.

use std::collections::HashMap;

use norad::Glyph;

use crate::outline::glyph_ops::{self, GlyphSnapshot};
use crate::ui::editing::undo::UndoState;

/// Undo and redo stacks for every glyph of one master.
#[derive(Debug, Clone, Default)]
pub struct EditHistory {
    stacks: HashMap<String, UndoState<GlyphSnapshot>>,
}

impl EditHistory {
    /// An empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the glyph's state before an edit, as a new undo step.
    /// Clears the glyph's redo stack, as any new edit does.
    pub fn record(&mut self, name: &str, glyph: &Glyph) {
        self.stack(name).add_undo_group(glyph_ops::snapshot(glyph));
    }

    /// Replaces the most recent step's snapshot without opening a new
    /// step. A drag records once on mouse-down and amends while it
    /// moves, so the whole drag is one undo.
    pub fn amend(&mut self, name: &str, glyph: &Glyph) {
        self.stack(name)
            .update_current_undo(glyph_ops::snapshot(glyph));
    }

    /// Drops the most recent step, for an edit that turned out to
    /// change nothing. Returns false when there was no step to drop.
    pub fn discard_last(&mut self, name: &str) -> bool {
        match self.stacks.get_mut(name) {
            Some(stack) => stack.discard_last(),
            None => false,
        }
    }

    /// Restores the previous state into `glyph`, pushing the current
    /// one onto redo. Returns false when there is nothing to undo.
    pub fn undo(&mut self, name: &str, glyph: &mut Glyph) -> bool {
        let Some(stack) = self.stacks.get_mut(name) else {
            return false;
        };
        match stack.undo(glyph_ops::snapshot(glyph)) {
            Some(previous) => {
                glyph_ops::restore(glyph, previous);
                true
            }
            None => false,
        }
    }

    /// Restores the next state into `glyph`, pushing the current one
    /// onto undo. Returns false when there is nothing to redo.
    pub fn redo(&mut self, name: &str, glyph: &mut Glyph) -> bool {
        let Some(stack) = self.stacks.get_mut(name) else {
            return false;
        };
        match stack.redo(glyph_ops::snapshot(glyph)) {
            Some(next) => {
                glyph_ops::restore(glyph, next);
                true
            }
            None => false,
        }
    }

    /// Whether the glyph has a step to undo.
    pub fn can_undo(&self, name: &str) -> bool {
        self.stacks.get(name).is_some_and(UndoState::can_undo)
    }

    /// Whether the glyph has a step to redo.
    pub fn can_redo(&self, name: &str) -> bool {
        self.stacks.get(name).is_some_and(UndoState::can_redo)
    }

    /// How many steps the glyph can undo.
    pub fn undo_depth(&self, name: &str) -> usize {
        self.stacks.get(name).map_or(0, UndoState::undo_depth)
    }

    /// Forgets one glyph's history, after it is renamed or removed.
    pub fn clear_glyph(&mut self, name: &str) {
        self.stacks.remove(name);
    }

    /// Forgets everything, after a reload from disk.
    pub fn clear(&mut self) {
        self.stacks.clear();
    }

    fn stack(&mut self, name: &str) -> &mut UndoState<GlyphSnapshot> {
        self.stacks.entry(name.to_string()).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Contour, ContourPoint, PointType};

    fn glyph(width: f64) -> Glyph {
        let mut g = Glyph::new("a");
        g.width = width;
        g.contours.push(Contour::new(
            vec![ContourPoint::new(
                0.0,
                0.0,
                PointType::Line,
                false,
                None,
                None,
            )],
            None,
        ));
        g
    }

    #[test]
    fn undo_and_redo_walk_the_pile() {
        let mut history = EditHistory::new();
        let mut g = glyph(100.0);
        history.record("a", &g);
        g.width = 200.0;
        history.record("a", &g);
        g.width = 300.0;

        assert!(history.undo("a", &mut g));
        assert_eq!(g.width, 200.0);
        assert!(history.undo("a", &mut g));
        assert_eq!(g.width, 100.0);
        assert!(!history.undo("a", &mut g));
        assert!(history.redo("a", &mut g));
        assert_eq!(g.width, 200.0);
        assert!(history.redo("a", &mut g));
        assert_eq!(g.width, 300.0);
        assert!(!history.redo("a", &mut g));
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let mut history = EditHistory::new();
        let mut g = glyph(100.0);
        history.record("a", &g);
        g.width = 200.0;
        history.undo("a", &mut g);
        assert!(history.can_redo("a"));
        history.record("a", &g);
        assert!(!history.can_redo("a"));
    }

    #[test]
    fn amend_keeps_a_drag_as_one_step() {
        let mut history = EditHistory::new();
        let mut g = glyph(100.0);
        history.record("a", &g);
        g.width = 150.0;
        history.amend("a", &g);
        g.width = 200.0;
        assert_eq!(history.undo_depth("a"), 1);
        history.undo("a", &mut g);
        assert_eq!(g.width, 150.0);
    }

    #[test]
    fn discard_drops_a_step_that_changed_nothing() {
        let mut history = EditHistory::new();
        let g = glyph(100.0);
        history.record("a", &g);
        assert!(history.discard_last("a"));
        assert!(!history.can_undo("a"));
        assert!(!history.discard_last("a"));
        assert!(!history.discard_last("b"));
    }

    #[test]
    fn glyphs_keep_separate_piles() {
        let mut history = EditHistory::new();
        let mut a = glyph(1.0);
        let mut b = glyph(2.0);
        history.record("a", &a);
        a.width = 10.0;
        assert!(!history.can_undo("b"));
        assert!(!history.undo("b", &mut b));
        assert_eq!(b.width, 2.0);
        history.clear_glyph("a");
        assert!(!history.can_undo("a"));
    }
}
