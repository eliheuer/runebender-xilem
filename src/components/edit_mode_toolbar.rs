// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Edit mode toolbar widget - tool selection toolbar for the editor view
//!
//! This toolbar displays icon-based buttons for selecting editing tools
//! (Select, Pen, Preview, etc.) and is shown in the editor view when
//! editing glyphs.

use crate::tools::ToolId;
use kurbo::{BezPath, Point, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget,
};
use masonry::vello::Scene;
use tracing;

// Import shared toolbar functionality
use crate::components::toolbars::{
    button_rect, calculate_toolbar_size, paint_button, paint_icon,
    paint_panel, ButtonState,
};

/// Available tools in display order
/// Currently only showing implemented tools: Select, Pen, Preview
const TOOLBAR_TOOLS: &[ToolId] = &[ToolId::Select, ToolId::Pen, ToolId::Preview];

/// Edit mode toolbar widget
pub struct EditModeToolbarWidget {
    /// Currently selected tool
    selected_tool: ToolId,
    /// Currently hovered tool (if any)
    hover_tool: Option<ToolId>,
}

impl EditModeToolbarWidget {
    pub fn new(selected_tool: ToolId) -> Self {
        Self {
            selected_tool,
            hover_tool: None,
        }
    }

    /// Get the icon path for a tool
    fn icon_for_tool(tool: ToolId) -> BezPath {
        match tool {
            ToolId::Select => select_icon(),
            ToolId::Pen => pen_icon(),
            ToolId::Preview => preview_icon(),
        }
    }


    /// Find which tool was clicked
    fn tool_at_point(&self, point: Point) -> Option<ToolId> {
        for (i, &tool) in TOOLBAR_TOOLS.iter().enumerate() {
            if button_rect(i).contains(point) {
                return Some(tool);
            }
        }
        None
    }
}

/// Action sent when a tool is selected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSelected(pub ToolId);

impl Widget for EditModeToolbarWidget {
    type Action = ToolSelected;

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
        let size = calculate_toolbar_size(TOOLBAR_TOOLS.len());
        bc.constrain(size)
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();

        // Draw background panel
        paint_panel(scene, size);

        // Draw each toolbar button
        for (i, &tool) in TOOLBAR_TOOLS.iter().enumerate() {
            let rect = button_rect(i);
            let is_selected = tool == self.selected_tool;
            let is_hovered = self.hover_tool == Some(tool);

            let state = ButtonState::new(is_hovered, is_selected);

            // Draw button background and border
            paint_button(scene, rect, state);

            // Draw icon
            let icon = Self::icon_for_tool(tool);
            paint_icon(scene, icon, rect, state);
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
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                tracing::debug!(
                    "[EditModeToolbarWidget::on_pointer_event] Down at {:?}",
                    state.position
                );
                let local_pos = ctx.local_position(state.position);
                tracing::debug!(
                    "[EditModeToolbarWidget::on_pointer_event] \
                     local_pos: {:?}",
                    local_pos
                );
                if let Some(tool) = self.tool_at_point(local_pos) {
                    tracing::debug!(
                        "[EditModeToolbarWidget::on_pointer_event] Hit \
                         tool: {:?}",
                        tool
                    );
                    if tool != self.selected_tool {
                        self.selected_tool = tool;
                        ctx.submit_action::<ToolSelected>(ToolSelected(tool));
                        ctx.request_render();
                    }
                    // Mark event as handled to prevent it from reaching
                    // widgets below in zstack
                    ctx.set_handled();
                } else {
                    // Even if we didn't hit a tool, consume the event so
                    // it doesn't go to the editor
                    ctx.set_handled();
                }
            }
            PointerEvent::Move(pointer_move) => {
                let local_pos =
                    ctx.local_position(pointer_move.current.position);
                let new_hover = self.tool_at_point(local_pos);
                if new_hover != self.hover_tool {
                    self.hover_tool = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_tool.is_some() {
                    self.hover_tool = None;
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


// --- Icon Definitions ---

fn select_icon() -> BezPath {
    // Icon from VirtuaGrotesk E010 (U+E010) - selection cursor
    // Y coordinates flipped to convert from UFO (Y-up) to screen (Y-down)
    let mut bez = BezPath::new();

    bez.move_to((328.0, 768.0));
    bez.curve_to((314.0, 768.0), (308.0, 762.0), (300.0, 734.0));
    bez.line_to((246.0, 542.0));
    bez.curve_to((240.0, 522.0), (236.0, 514.0), (224.0, 514.0));
    bez.curve_to((210.0, 514.0), (202.0, 520.0), (192.0, 530.0));
    bez.line_to((138.0, 584.0));
    bez.curve_to((122.0, 600.0), (108.0, 608.0), (90.0, 608.0));
    bez.curve_to((72.0, 608.0), (66.0, 598.0), (66.0, 574.0));
    bez.line_to((64.0, 50.0));
    bez.curve_to((64.0, 18.0), (78.0, 0.0), (96.0, 0.0));
    bez.curve_to((120.0, 0.0), (142.0, 16.0), (176.0, 50.0));
    bez.line_to((506.0, 368.0));
    bez.curve_to((526.0, 386.0), (540.0, 404.0), (540.0, 422.0));
    bez.curve_to((540.0, 440.0), (528.0, 450.0), (502.0, 450.0));
    bez.line_to((388.0, 450.0));
    bez.curve_to((368.0, 450.0), (360.0, 458.0), (360.0, 470.0));
    bez.curve_to((360.0, 484.0), (370.0, 496.0), (378.0, 510.0));
    bez.line_to((450.0, 634.0));
    bez.curve_to((460.0, 650.0), (478.0, 674.0), (478.0, 688.0));
    bez.curve_to((478.0, 706.0), (462.0, 714.0), (444.0, 722.0));
    bez.line_to((366.0, 760.0));
    bez.curve_to((352.0, 766.0), (344.0, 768.0), (328.0, 768.0));
    bez.close_path();

    bez
}

fn pen_icon() -> BezPath {
    // Icon from VirtuaGrotesk E011 (U+E011) - pen tool
    // Y coordinates flipped to convert from UFO (Y-up) to screen (Y-down)
    let mut bez = BezPath::new();

    // Contour 1 - top rectangle (nib)
    bez.move_to((200.0, 768.0));
    bez.line_to((432.0, 768.0));
    bez.curve_to((452.0, 768.0), (456.0, 764.0), (456.0, 744.0));
    bez.line_to((456.0, 678.0));
    bez.curve_to((456.0, 658.0), (452.0, 654.0), (432.0, 654.0));
    bez.line_to((200.0, 654.0));
    bez.curve_to((180.0, 654.0), (176.0, 658.0), (176.0, 678.0));
    bez.line_to((176.0, 744.0));
    bez.curve_to((176.0, 764.0), (180.0, 768.0), (200.0, 768.0));
    bez.close_path();

    // Contour 2 - pen body
    bez.move_to((200.0, 602.0));
    bez.line_to((432.0, 602.0));
    bez.curve_to((454.0, 602.0), (460.0, 604.0), (480.0, 576.0));
    bez.line_to((548.0, 484.0));
    bez.curve_to((556.0, 472.0), (564.0, 462.0), (564.0, 452.0));
    bez.line_to((564.0, 416.0));
    bez.curve_to((564.0, 410.0), (560.0, 396.0), (556.0, 384.0));
    bez.line_to((440.0, 32.0));
    bez.curve_to((430.0, 0.0), (416.0, 0.0), (400.0, 0.0));
    bez.line_to((364.0, 0.0));
    bez.curve_to((348.0, 0.0), (342.0, 8.0), (342.0, 32.0));
    bez.line_to((342.0, 336.0));
    bez.curve_to((342.0, 358.0), (346.0, 362.0), (352.0, 366.0));
    bez.curve_to((374.0, 378.0), (392.0, 400.0), (392.0, 434.0));
    bez.curve_to((392.0, 478.0), (360.0, 510.0), (316.0, 510.0));
    bez.curve_to((272.0, 510.0), (240.0, 478.0), (240.0, 434.0));
    bez.curve_to((240.0, 400.0), (258.0, 378.0), (280.0, 366.0));
    bez.curve_to((286.0, 362.0), (290.0, 358.0), (290.0, 336.0));
    bez.line_to((290.0, 32.0));
    bez.curve_to((290.0, 8.0), (284.0, 0.0), (268.0, 0.0));
    bez.line_to((232.0, 0.0));
    bez.curve_to((216.0, 0.0), (202.0, 0.0), (192.0, 32.0));
    bez.line_to((76.0, 384.0));
    bez.curve_to((72.0, 396.0), (68.0, 410.0), (68.0, 416.0));
    bez.line_to((68.0, 452.0));
    bez.curve_to((68.0, 462.0), (76.0, 472.0), (84.0, 484.0));
    bez.line_to((152.0, 576.0));
    bez.curve_to((172.0, 602.0), (180.0, 602.0), (200.0, 602.0));
    bez.close_path();

    bez
}

fn preview_icon() -> BezPath {
    // Icon from VirtuaGrotesk E014 (U+E014) - preview/hand tool
    // Y coordinates flipped to convert from UFO (Y-up) to screen (Y-down)
    let mut bez = BezPath::new();

    bez.move_to((256.0, 798.0));
    bez.line_to((240.0, 798.0));
    bez.curve_to((232.0, 788.0), (232.0, 774.0), (232.0, 774.0));
    bez.line_to((232.0, 726.0));
    bez.curve_to((232.0, 714.0), (226.0, 704.0), (208.0, 686.0));
    bez.curve_to((128.0, 606.0), (90.0, 466.0), (90.0, 272.0));
    bez.curve_to((90.0, 202.0), (114.0, 168.0), (138.0, 168.0));
    bez.curve_to((152.0, 168.0), (158.0, 178.0), (158.0, 192.0));
    bez.curve_to((158.0, 208.0), (154.0, 224.0), (154.0, 264.0));
    bez.curve_to((154.0, 290.0), (168.0, 356.0), (182.0, 384.0));
    bez.curve_to((186.0, 392.0), (194.0, 394.0), (200.0, 394.0));
    bez.curve_to((206.0, 394.0), (212.0, 392.0), (212.0, 384.0));
    bez.curve_to((212.0, 372.0), (200.0, 332.0), (200.0, 296.0));
    bez.curve_to((200.0, 194.0), (230.0, 56.0), (266.0, 56.0));
    bez.curve_to((302.0, 56.0), (298.0, 80.0), (298.0, 92.0));
    bez.curve_to((298.0, 110.0), (286.0, 136.0), (286.0, 222.0));
    bez.curve_to((286.0, 292.0), (290.0, 318.0), (292.0, 326.0));
    bez.curve_to((294.0, 334.0), (302.0, 340.0), (308.0, 340.0));
    bez.curve_to((314.0, 340.0), (322.0, 334.0), (322.0, 326.0));
    bez.curve_to((322.0, 174.0), (370.0, 66.0), (396.0, 30.0));
    bez.curve_to((412.0, 8.0), (428.0, 0.0), (450.0, 0.0));
    bez.curve_to((462.0, 0.0), (470.0, 12.0), (470.0, 30.0));
    bez.curve_to((470.0, 54.0), (416.0, 118.0), (416.0, 272.0));
    bez.curve_to((416.0, 298.0), (416.0, 318.0), (418.0, 324.0));
    bez.curve_to((420.0, 330.0), (424.0, 332.0), (428.0, 332.0));
    bez.curve_to((432.0, 332.0), (440.0, 328.0), (442.0, 322.0));
    bez.curve_to((470.0, 194.0), (518.0, 122.0), (552.0, 90.0));
    bez.curve_to((566.0, 76.0), (578.0, 72.0), (592.0, 72.0));
    bez.curve_to((606.0, 72.0), (610.0, 82.0), (610.0, 98.0));
    bez.curve_to((610.0, 118.0), (522.0, 268.0), (522.0, 406.0));
    bez.curve_to((522.0, 464.0), (558.0, 490.0), (582.0, 490.0));
    bez.curve_to((612.0, 490.0), (638.0, 442.0), (660.0, 402.0));
    bez.curve_to((686.0, 356.0), (708.0, 336.0), (734.0, 336.0));
    bez.curve_to((748.0, 336.0), (756.0, 344.0), (756.0, 362.0));
    bez.curve_to((756.0, 402.0), (668.0, 668.0), (518.0, 734.0));
    bez.curve_to((500.0, 742.0), (490.0, 752.0), (490.0, 764.0));
    bez.line_to((490.0, 774.0));
    bez.curve_to((490.0, 790.0), (484.0, 798.0), (470.0, 798.0));
    bez.line_to((256.0, 798.0));
    bez.close_path();

    bez
}

#[allow(dead_code)]
fn knife_icon() -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to((30.0, 500.0));
    bez.line_to((190.0, 500.0));
    bez.line_to((190.0, 410.0));
    bez.line_to((30.0, 410.0));
    bez.line_to((30.0, 500.0));
    bez.close_path();
    bez.move_to((40.0, 360.0));
    bez.line_to((180.0, 360.0));
    bez.line_to((180.0, 330.0));
    bez.line_to((220.0, 290.0));
    bez.line_to((42.0, 0.0));
    bez.line_to((40.0, 0.0));
    bez.line_to((40.0, 360.0));
    bez.close_path();
    bez.move_to((30.0, 410.0));
    bez.line_to((190.0, 410.0));
    bez.curve_to((205.0, 410.0), (220.0, 405.0), (220.0, 385.0));
    bez.curve_to((220.0, 365.0), (205.0, 360.0), (190.0, 360.0));
    bez.line_to((30.0, 360.0));
    bez.curve_to((15.0, 360.0), (0.0, 365.0), (0.0, 385.0));
    bez.curve_to((0.0, 405.0), (15.0, 410.0), (30.0, 410.0));
    bez.close_path();
    bez
}

#[allow(dead_code)]
fn rect_icon() -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to((0.0, 500.0));
    bez.line_to((220.0, 500.0));
    bez.line_to((220.0, 0.0));
    bez.line_to((0.0, 0.0));
    bez.line_to((0.0, 500.0));
    bez.close_path();
    bez
}

#[allow(dead_code)]
fn ellipse_icon() -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to((110.0, 0.0));
    bez.curve_to((50.0, 0.0), (0.0, 100.0), (0.0, 240.0));
    bez.curve_to((0.0, 380.0), (50.0, 480.0), (110.0, 480.0));
    bez.curve_to((170.0, 480.0), (220.0, 380.0), (220.0, 240.0));
    bez.curve_to((220.0, 100.0), (170.0, 0.0), (110.0, 0.0));
    bez.close_path();
    bez
}

// ===== XILEM VIEW WRAPPER =====

use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Create an edit mode toolbar view
pub fn edit_mode_toolbar_view<State, Action>(
    selected_tool: ToolId,
    callback: impl Fn(&mut State, ToolId) + Send + Sync + 'static,
) -> EditModeToolbarView<State, Action>
where
    Action: 'static,
{
    EditModeToolbarView {
        selected_tool,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

/// The Xilem View for EditModeToolbarWidget
/// Callback type for toolbar button clicks
type EditModeToolbarCallback<State> =
    Box<dyn Fn(&mut State, ToolId) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct EditModeToolbarView<State, Action = ()> {
    selected_tool: ToolId,
    callback: EditModeToolbarCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for EditModeToolbarView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for EditModeToolbarView<State, Action>
{
    type Element = Pod<EditModeToolbarWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = EditModeToolbarWidget::new(self.selected_tool);
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
        // Update widget if selected tool changed
        let mut widget = element.downcast::<EditModeToolbarWidget>();
        if widget.widget.selected_tool != self.selected_tool {
            widget.widget.selected_tool = self.selected_tool;
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
        // Handle tool selection actions from widget
        match message.take_message::<ToolSelected>() {
            Some(action) => {
                tracing::debug!(
                    "[EditModeToolbarView::message] Tool selected: {:?}",
                    action.0
                );
                (self.callback)(app_state, action.0);
                // Return Action to trigger full app rebuild so the toolbar
                // gets the updated tool. This causes app_logic() to be
                // called, which reads the fresh tool from the session
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

#[allow(dead_code)]
fn measure_icon() -> BezPath {
    let mut bez = BezPath::new();
    bez.move_to((0.0, 500.0));
    bez.line_to((140.0, 500.0));
    bez.line_to((140.0, 0.0));
    bez.line_to((0.0, 0.0));
    bez.line_to((0.0, 500.0));
    bez.close_path();
    bez.move_to((190.0, 0.0));
    bez.line_to((330.0, 0.0));
    bez.move_to((190.0, 500.0));
    bez.line_to((330.0, 500.0));
    bez.move_to((210.0, 100.0));
    bez.line_to((310.0, 100.0));
    bez.line_to((260.0, 10.0));
    bez.line_to((210.0, 100.0));
    bez.close_path();
    bez.move_to((210.0, 400.0));
    bez.line_to((310.0, 400.0));
    bez.line_to((260.0, 490.0));
    bez.line_to((210.0, 400.0));
    bez.close_path();
    bez.move_to((260.0, 100.0));
    bez.line_to((260.0, 400.0));
    bez.move_to((70.0, 350.0));
    bez.line_to((140.0, 350.0));
    bez.move_to((100.0, 400.0));
    bez.line_to((140.0, 400.0));
    bez.move_to((50.0, 450.0));
    bez.line_to((140.0, 450.0));
    bez.move_to((100.0, 300.0));
    bez.line_to((140.0, 300.0));
    bez.move_to((50.0, 250.0));
    bez.line_to((140.0, 250.0));
    bez.move_to((70.0, 150.0));
    bez.line_to((140.0, 150.0));
    bez.move_to((100.0, 200.0));
    bez.line_to((140.0, 200.0));
    bez.move_to((100.0, 100.0));
    bez.line_to((140.0, 100.0));
    bez.move_to((50.0, 50.0));
    bez.line_to((140.0, 50.0));
    bez
}

