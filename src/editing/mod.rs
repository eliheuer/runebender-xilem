// Subset of runebender-xilem's `editing/` that's free of kurbo
// geometry types — the rest (viewport, hit_test, mouse) stays in
// each consumer until they can share a kurbo version.

pub mod edit_types;
pub mod selection;
pub mod undo;

pub use edit_types::EditType;
pub use selection::Selection;
pub use undo::UndoState;
