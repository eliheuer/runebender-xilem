// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Master toolbar widget - switch between masters in a designspace project
//!
//! This toolbar appears when a designspace file is loaded, allowing users
//! to switch between font masters. Each button shows the "n" glyph rendered
//! from that master.

use crate::glyph_renderer::glyph_to_bezpath_with_components;
use crate::workspace::Workspace;
use kurbo::{Affine, BezPath, Point, Rect, Shape, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget,
};
use masonry::util::fill_color;
use masonry::vello::Scene;
use std::marker::PhantomData;
use tracing;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

// Import shared toolbar functionality
use crate::components::toolbars::{
    button_rect, calculate_toolbar_size, paint_button, paint_panel, ButtonState,
};

use crate::theme::toolbar::{ICON_SELECTED, ICON_UNSELECTED, ICON_HOVERED};
use crate::theme::size::{TOOLBAR_ICON_PADDING, TOOLBAR_ITEM_SIZE};

/// Glyph to use for master preview (lowercase n is good for showing weight)
const PREVIEW_GLYPH: &str = "n";

/// Info about a master needed for the toolbar
#[derive(Clone, Debug)]
pub struct MasterInfo {
    /// Master index
    pub index: usize,
    /// Master display name
    pub name: String,
    /// Style name (e.g., "Regular", "Bold")
    pub style_name: String,
    /// Pre-rendered BezPath of the preview glyph
    pub preview_path: Option<BezPath>,
}

/// Master toolbar widget
pub struct MasterToolbarWidget {
    /// Info about each master
    masters: Vec<MasterInfo>,
    /// Currently active master index
    active_master: usize,
    /// Currently hovered master index (if any)
    hover_index: Option<usize>,
}

impl MasterToolbarWidget {
    pub fn new(masters: Vec<MasterInfo>, active_master: usize) -> Self {
        Self {
            masters,
            active_master,
            hover_index: None,
        }
    }

    /// Find which master was clicked
    fn master_at_point(&self, point: Point) -> Option<usize> {
        for i in 0..self.masters.len() {
            if button_rect(i).contains(point) {
                return Some(i);
            }
        }
        None
    }

    /// Paint a glyph icon in a button
    fn paint_glyph_icon(
        scene: &mut Scene,
        path: &BezPath,
        button_rect: Rect,
        state: ButtonState,
    ) {
        let icon_bounds = path.bounding_box();
        if icon_bounds.width() <= 0.0 || icon_bounds.height() <= 0.0 {
            return;
        }

        let icon_center = icon_bounds.center();
        let button_center = button_rect.center();

        // Scale glyph to fit with padding
        let icon_size = icon_bounds.width().max(icon_bounds.height());
        let target_size = TOOLBAR_ITEM_SIZE - TOOLBAR_ICON_PADDING * 2.0;
        let scale = target_size / icon_size;

        // Create transform: scale then translate to center
        let transform = Affine::translate((button_center.x, button_center.y))
            * Affine::scale(scale)
            * Affine::translate((-icon_center.x, -icon_center.y));

        // Determine icon color based on state
        let icon_color = if state.is_selected {
            ICON_SELECTED
        } else if state.is_hovered {
            ICON_HOVERED
        } else {
            ICON_UNSELECTED
        };

        fill_color(scene, &(transform * path), icon_color);
    }
}

/// Action sent when a master is selected
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MasterSelected(pub usize);

impl Widget for MasterToolbarWidget {
    type Action = MasterSelected;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // No update logic needed
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let size = calculate_toolbar_size(self.masters.len());
        bc.constrain(size)
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();
        paint_panel(scene, size);

        // Paint each master button
        for (i, master) in self.masters.iter().enumerate() {
            let rect = button_rect(i);
            let state = ButtonState::new(
                self.hover_index == Some(i),
                self.active_master == i,
            );

            paint_button(scene, rect, state);

            // Paint the preview glyph if available
            if let Some(ref path) = master.preview_path {
                Self::paint_glyph_icon(scene, path, rect, state);
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Toolbar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
        // TODO: Add accessibility info
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(state) => {
                let local_pos = ctx.local_position(state.current.position);
                let new_hover = self.master_at_point(local_pos);
                if new_hover != self.hover_index {
                    self.hover_index = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let local_pos = ctx.local_position(state.position);
                if let Some(index) = self.master_at_point(local_pos) {
                    if index != self.active_master {
                        tracing::debug!("Master toolbar: clicked master {}", index);
                        self.active_master = index;
                        ctx.request_render();
                        ctx.submit_action::<MasterSelected>(MasterSelected(index));
                    }
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_index.is_some() {
                    self.hover_index = None;
                    ctx.request_render();
                }
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
        // No text handling needed
    }
}

// --- Helper Functions ---

/// Generate preview path for a master's "n" glyph
pub fn generate_master_preview(workspace: &Workspace) -> Option<BezPath> {
    let glyph = workspace.get_glyph(PREVIEW_GLYPH)?;
    let path = glyph_to_bezpath_with_components(glyph, workspace);
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Create MasterInfo list from a designspace project
pub fn create_master_infos(
    masters: &[crate::designspace::Master],
) -> Vec<MasterInfo> {
    masters
        .iter()
        .enumerate()
        .map(|(index, master)| {
            let preview_path = master
                .workspace
                .read()
                .ok()
                .and_then(|ws| generate_master_preview(&ws));

            MasterInfo {
                index,
                name: master.name.clone(),
                style_name: master.style_name.clone(),
                preview_path,
            }
        })
        .collect()
}

// --- Xilem View Wrapper ---

/// Public API to create a master toolbar view
pub fn master_toolbar_view<State, Action>(
    masters: Vec<MasterInfo>,
    active_master: usize,
    callback: impl Fn(&mut State, usize) + Send + Sync + 'static,
) -> MasterToolbarView<State, Action>
where
    Action: 'static,
{
    MasterToolbarView {
        masters,
        active_master,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

/// The Xilem View for MasterToolbarWidget
type MasterToolbarCallback<State> =
    Box<dyn Fn(&mut State, usize) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct MasterToolbarView<State, Action = ()> {
    masters: Vec<MasterInfo>,
    active_master: usize,
    callback: MasterToolbarCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for MasterToolbarView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for MasterToolbarView<State, Action>
{
    type Element = Pod<MasterToolbarWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = MasterToolbarWidget::new(
            self.masters.clone(),
            self.active_master,
        );
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let mut widget = element.downcast::<MasterToolbarWidget>();

        // Update active master if changed
        if widget.widget.active_master != self.active_master {
            widget.widget.active_master = self.active_master;
            widget.ctx.request_render();
        }

        // Update masters list if changed (e.g., different designspace loaded)
        if widget.widget.masters.len() != self.masters.len() {
            widget.widget.masters = self.masters.clone();
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
    ) -> MessageResult<Action> {
        // Handle master selection actions from widget
        match message.take_message::<MasterSelected>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}
