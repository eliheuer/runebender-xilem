// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Xilem View wrapper for EditorWidget

use super::{EditorWidget, SessionUpdate};
use crate::editing::EditSession;
use std::marker::PhantomData;
use std::sync::Arc;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Create an editor view from an edit session with a callback for
/// session updates
///
/// The callback receives the updated session and a boolean indicating
/// whether save was requested (Cmd+S).
pub fn editor_view<State, F>(
    session: Arc<EditSession>,
    on_session_update: F,
) -> EditorView<State, F>
where
    F: Fn(&mut State, EditSession, bool),
{
    EditorView {
        session,
        on_session_update,
        phantom: PhantomData,
    }
}

/// The Xilem View for EditorWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct EditorView<State, F> {
    session: Arc<EditSession>,
    on_session_update: F,
    phantom: PhantomData<fn() -> State>,
}

impl<State, F> ViewMarker for EditorView<State, F> {}

impl<State: 'static, F: Fn(&mut State, EditSession, bool) + 'static> View<State, (), ViewCtx>
    for EditorView<State, F>
{
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = EditorWidget::new(self.session.clone());
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Update the widget's session if it changed (e.g., tool
        // selection changed). We compare Arc pointers - if they're
        // different, the session was updated
        if !Arc::ptr_eq(&self.session, &prev.session) {
            tracing::debug!(
                "[EditorView::rebuild] Session Arc changed, \
                 updating widget"
            );
            tracing::debug!(
                "[EditorView::rebuild] Old tool: {:?}, New tool: \
                 {:?}",
                prev.session.current_tool.id(),
                self.session.current_tool.id()
            );

            // Get mutable access to the widget
            let mut widget = element.downcast::<EditorWidget>();

            // Preserve viewport state before updating session
            let old_viewport = widget.widget.session.viewport.clone();
            let old_viewport_initialized = widget.widget.session.viewport_initialized;

            // Update the session, but preserve:
            // - Mouse state (to avoid breaking active drag
            //   operations)
            // - Undo state
            // - Canvas size
            // - Viewport state (to avoid re-initialization and flickering)
            // This allows tool changes and other session updates to
            // take effect
            widget.widget.session = (*self.session).clone();

            // Restore viewport state
            widget.widget.session.viewport = old_viewport;
            widget.widget.session.viewport_initialized = old_viewport_initialized;

            widget.ctx.request_render();
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // No cleanup needed
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        // Handle SessionUpdate messages from the widget
        match message.take_message::<SessionUpdate>() {
            Some(update) => {
                tracing::debug!(
                    "[EditorView::message] Handling SessionUpdate, \
                     calling callback, selection.len()={}, save_requested={}",
                    update.session.selection.len(),
                    update.save_requested
                );
                (self.on_session_update)(app_state, update.session, update.save_requested);
                tracing::debug!(
                    "[EditorView::message] Callback complete, \
                     returning Action(())"
                );
                // Return Action(()) to propagate to root and trigger
                // full app rebuild. This ensures all UI elements
                // (including coordinate pane) see the updated state
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}
