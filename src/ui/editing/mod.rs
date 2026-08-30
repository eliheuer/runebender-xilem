//! Editing state every front-end shares: selection, undo, and the
//! viewport. Started as the toolkit-free subset of runebender-xilem's
//! `editing/`.

pub mod edit_types;
pub mod selection;
pub mod undo;
pub mod viewport;

pub use edit_types::EditType;
pub use selection::Selection;
pub use undo::UndoState;
pub use viewport::ViewPort;
