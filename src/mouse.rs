// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Mouse event handling and state machine
//!
//! This module provides a state machine for tracking mouse events and
//! converting them into high-level gestures (clicks, drags, etc.).

use kurbo::Point;

/// Threshold distance (in screen pixels) before a drag is recognized
const DRAG_THRESHOLD: f64 = 3.0;

/// Mouse button states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MouseButton {
    Left,
    Right,
    Other,
}

/// Modifier keys state
#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// A mouse event with position
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    /// Position in screen coordinates
    pub pos: Point,
    /// Which button (if any)
    pub button: Option<MouseButton>,
    /// Modifier keys
    pub mods: Modifiers,
}

#[allow(dead_code)]
impl MouseEvent {
    /// Create a new mouse event
    pub fn new(pos: Point, button: Option<MouseButton>) -> Self {
        Self {
            pos,
            button,
            mods: Modifiers::default(),
        }
    }

    /// Create a new mouse event with modifiers
    pub fn with_modifiers(
        pos: Point,
        button: Option<MouseButton>,
        mods: Modifiers,
    ) -> Self {
        Self { pos, button, mods }
    }
}

/// Information about a drag gesture
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Drag {
    /// Where the drag started
    pub start: Point,
    /// Previous mouse position
    pub prev: Point,
    /// Current mouse position
    pub current: Point,
}

impl Drag {
    /// Get the delta from start to current
    #[allow(dead_code)]
    pub fn delta_from_start(&self) -> (f64, f64) {
        (self.current.x - self.start.x, self.current.y - self.start.y)
    }

    /// Get the delta from previous to current
    #[allow(dead_code)]
    pub fn delta_from_prev(&self) -> (f64, f64) {
        (self.current.x - self.prev.x, self.current.y - self.prev.y)
    }
}

/// Mouse state machine internal state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseState {
    /// No buttons pressed
    Up,
    /// Button pressed but not dragging yet
    Down,
    /// Dragging (moved beyond threshold)
    Drag,
}

/// Mouse state machine
///
/// Tracks mouse events and converts them into high-level gestures.
pub struct Mouse {
    /// Current state
    state: MouseState,
    /// Button that is currently down (if any)
    current_button: Option<MouseButton>,
    /// Position where button was pressed
    down_pos: Point,
    /// Last known mouse position
    last_pos: Point,
}

impl Mouse {
    /// Create a new mouse state machine
    pub fn new() -> Self {
        Self {
            state: MouseState::Up,
            current_button: None,
            down_pos: Point::ZERO,
            last_pos: Point::ZERO,
        }
    }

    /// Handle a mouse down event
    pub fn mouse_down<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        match self.state {
            MouseState::Up => {
                self.handle_button_down(event, delegate, data);
            }
            _ => {
                // Ignore if already down or dragging
            }
        }
    }

    /// Handle button down when in Up state
    fn handle_button_down<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        self.state = MouseState::Down;
        self.current_button = event.button;
        self.down_pos = event.pos;
        self.last_pos = event.pos;

        // Call appropriate delegate method
        Self::call_button_down(
            event.button,
            delegate,
            event,
            data,
        );
    }

    /// Call the appropriate button down delegate method
    fn call_button_down<T: MouseDelegate>(
        button: Option<MouseButton>,
        delegate: &mut T,
        event: MouseEvent,
        data: &mut T::Data,
    ) {
        match button {
            Some(MouseButton::Left) => {
                delegate.left_down(event, data);
            }
            Some(MouseButton::Right) => {
                delegate.right_down(event, data);
            }
            Some(MouseButton::Other) => {
                delegate.other_down(event, data);
            }
            None => {}
        }
    }

    /// Handle a mouse up event
    pub fn mouse_up<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        match self.state {
            MouseState::Down => {
                self.handle_click_up(event, delegate, data);
            }
            MouseState::Drag => {
                self.handle_drag_up(event, delegate, data);
            }
            MouseState::Up => {
                // Ignore up event when already up
            }
        }
    }

    /// Handle button up after a click (not a drag)
    fn handle_click_up<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        Self::call_click_up(event.button, delegate, event, data);
        self.reset_state();
    }

    /// Handle button up after a drag
    fn handle_drag_up<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        let drag = Self::create_drag(
            self.down_pos,
            self.last_pos,
            event.pos,
        );

        Self::call_drag_ended(
            self.current_button,
            delegate,
            event,
            drag,
            data,
        );
        self.reset_state();
    }

    /// Reset mouse state to Up
    fn reset_state(&mut self) {
        self.state = MouseState::Up;
        self.current_button = None;
    }

    /// Call the appropriate click up delegate methods
    fn call_click_up<T: MouseDelegate>(
        button: Option<MouseButton>,
        delegate: &mut T,
        event: MouseEvent,
        data: &mut T::Data,
    ) {
        match button {
            Some(MouseButton::Left) => {
                delegate.left_up(event, data);
                delegate.left_click(event, data);
            }
            Some(MouseButton::Right) => {
                delegate.right_up(event, data);
                delegate.right_click(event, data);
            }
            Some(MouseButton::Other) => {
                delegate.other_up(event, data);
                delegate.other_click(event, data);
            }
            None => {}
        }
    }

    /// Call the appropriate drag ended delegate methods
    fn call_drag_ended<T: MouseDelegate>(
        button: Option<MouseButton>,
        delegate: &mut T,
        event: MouseEvent,
        drag: Drag,
        data: &mut T::Data,
    ) {
        match button {
            Some(MouseButton::Left) => {
                delegate.left_drag_ended(event, drag, data);
                delegate.left_up(event, data);
            }
            Some(MouseButton::Right) => {
                delegate.right_drag_ended(event, drag, data);
                delegate.right_up(event, data);
            }
            Some(MouseButton::Other) => {
                delegate.other_drag_ended(event, drag, data);
                delegate.other_up(event, data);
            }
            None => {}
        }
    }

    /// Create a Drag struct from positions
    fn create_drag(
        start: Point,
        prev: Point,
        current: Point,
    ) -> Drag {
        Drag {
            start,
            prev,
            current,
        }
    }

    /// Handle a mouse move event
    pub fn mouse_moved<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        match self.state {
            MouseState::Up => {
                self.handle_move_without_button(event, delegate, data);
            }
            MouseState::Down => {
                self.handle_move_while_down(event, delegate, data);
            }
            MouseState::Drag => {
                self.handle_move_while_dragging(event, delegate, data);
            }
        }
    }

    /// Handle mouse move with no button pressed
    fn handle_move_without_button<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        self.last_pos = event.pos;
        delegate.mouse_moved(event, data);
    }

    /// Handle mouse move while button is down (may start drag)
    fn handle_move_while_down<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        if Self::should_start_drag(self.down_pos, event.pos) {
            self.start_drag(event, delegate, data);
        }
        self.last_pos = event.pos;
    }

    /// Handle mouse move while dragging
    fn handle_move_while_dragging<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        let drag = Self::create_drag(
            self.down_pos,
            self.last_pos,
            event.pos,
        );

        Self::call_drag_changed(
            self.current_button,
            delegate,
            event,
            drag,
            data,
        );

        self.last_pos = event.pos;
    }

    /// Check if mouse has moved far enough to start a drag
    fn should_start_drag(down_pos: Point, current_pos: Point) -> bool {
        let delta_x = current_pos.x - down_pos.x;
        let delta_y = current_pos.y - down_pos.y;
        let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
        distance >= DRAG_THRESHOLD
    }

    /// Start a drag gesture
    fn start_drag<T: MouseDelegate>(
        &mut self,
        event: MouseEvent,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        self.state = MouseState::Drag;

        let drag = Self::create_drag(
            self.down_pos,
            self.last_pos,
            event.pos,
        );

        Self::call_drag_began(
            self.current_button,
            delegate,
            event,
            drag,
            data,
        );
    }

    /// Call the appropriate drag began delegate method
    fn call_drag_began<T: MouseDelegate>(
        button: Option<MouseButton>,
        delegate: &mut T,
        event: MouseEvent,
        drag: Drag,
        data: &mut T::Data,
    ) {
        match button {
            Some(MouseButton::Left) => {
                delegate.left_drag_began(event, drag, data);
            }
            Some(MouseButton::Right) => {
                delegate.right_drag_began(event, drag, data);
            }
            Some(MouseButton::Other) => {
                delegate.other_drag_began(event, drag, data);
            }
            None => {}
        }
    }

    /// Call the appropriate drag changed delegate method
    fn call_drag_changed<T: MouseDelegate>(
        button: Option<MouseButton>,
        delegate: &mut T,
        event: MouseEvent,
        drag: Drag,
        data: &mut T::Data,
    ) {
        match button {
            Some(MouseButton::Left) => {
                delegate.left_drag_changed(event, drag, data);
            }
            Some(MouseButton::Right) => {
                delegate.right_drag_changed(event, drag, data);
            }
            Some(MouseButton::Other) => {
                delegate.other_drag_changed(event, drag, data);
            }
            None => {}
        }
    }

    /// Cancel any ongoing gesture
    pub fn cancel<T: MouseDelegate>(
        &mut self,
        delegate: &mut T,
        data: &mut T::Data,
    ) {
        delegate.cancel(data);
        self.reset_state();
    }

    /// Get the current mouse position
    #[allow(dead_code)]
    pub fn pos(&self) -> Point {
        self.last_pos
    }
}

impl Default for Mouse {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for handling high-level mouse events
///
/// Tools implement this trait to respond to clicks, drags, etc.
pub trait MouseDelegate {
    /// The data type that mouse events operate on
    type Data;

    /// Left mouse button pressed
    fn left_down(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Left mouse button released
    fn left_up(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Left mouse button clicked (down and up without drag)
    fn left_click(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Left mouse drag began (moved beyond threshold)
    fn left_drag_began(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Left mouse drag continued (mouse moved while dragging)
    fn left_drag_changed(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Left mouse drag ended (button released after drag)
    fn left_drag_ended(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Right mouse button pressed
    fn right_down(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Right mouse button released
    fn right_up(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Right mouse button clicked
    fn right_click(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Right mouse drag began
    fn right_drag_began(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Right mouse drag continued
    fn right_drag_changed(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Right mouse drag ended
    fn right_drag_ended(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Other mouse button pressed
    fn other_down(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Other mouse button released
    fn other_up(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Other mouse button clicked
    fn other_click(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Other mouse drag began
    fn other_drag_began(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Other mouse drag continued
    fn other_drag_changed(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Other mouse drag ended
    fn other_drag_ended(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
    }

    /// Mouse moved (no button pressed)
    fn mouse_moved(&mut self, _event: MouseEvent, _data: &mut Self::Data) {}

    /// Cancel any ongoing gesture
    fn cancel(&mut self, _data: &mut Self::Data) {}
}
