# Global Keyboard Shortcuts in Xilem ZStack

## Problem

When building a multi-view application with xilem using `zstack` for layered UI (floating toolbars, panels over a main content area), there's no straightforward way to implement global keyboard shortcuts like Cmd+S that work across the entire view.

## Current Workaround

We created a custom `KeyboardShortcutsWidget` that:
1. Takes up the full size of its container
2. Requests focus on pointer move/down events
3. Handles keyboard events (like Cmd+S) when focused

```rust
pub struct KeyboardShortcutsWidget {
    size: Size,
}

impl Widget for KeyboardShortcutsWidget {
    // ...
    fn on_pointer_event(&mut self, ctx: &mut EventCtx<'_>, ...) {
        match event {
            PointerEvent::Down(_) | PointerEvent::Move(_) => {
                ctx.request_focus();
            }
            _ => {}
        }
    }

    fn on_text_event(&mut self, ctx: &mut EventCtx<'_>, ...) {
        if let TextEvent::Keyboard(key_event) = event {
            let cmd = key_event.modifiers.meta() || key_event.modifiers.ctrl();
            if cmd && matches!(&key_event.key, Key::Character(c) if c == "s") {
                ctx.submit_action::<SaveRequested>(SaveRequested);
                ctx.set_handled();
            }
        }
    }
}
```

## Issues with this approach

### Position in ZStack matters

- **At bottom of zstack**: The keyboard handler only receives pointer events in areas not covered by widgets above it. Keyboard shortcuts only work when the pointer is over "empty" background areas.

- **At top of zstack**: The keyboard handler intercepts all pointer events first, breaking interaction with widgets below (buttons don't work, scrolling breaks, etc.), even when not calling `set_handled()`.

### No way to have "transparent" keyboard handling

There doesn't seem to be a way to create a widget that:
- Receives keyboard events globally
- Doesn't interfere with pointer event handling for widgets in the same zstack

## Desired Solution

Ideally, xilem/masonry would provide one of:

1. **Window-level keyboard event handling** - A way to register keyboard shortcuts at the window level that fire regardless of which widget has focus.

2. **Transparent overlay widgets** - A widget type that can receive keyboard events when focused but is completely transparent to pointer event hit-testing.

3. **Keyboard event bubbling** - A mechanism where keyboard events bubble up through the widget tree, allowing parent containers to handle shortcuts that children don't consume.

4. **View modifier for keyboard shortcuts** - Something like:
   ```rust
   my_view
       .on_key(Key::Character("s"), Modifiers::META, |state| {
           state.save();
       })
   ```

## Environment

- xilem 0.4
- masonry 0.4
- Rust 1.88

## Related Code

See `src/components/keyboard_handler.rs` and `src/views/glyph_grid.rs` in runebender-xilem for the current implementation.
