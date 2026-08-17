// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! UI components for the Runebender Xilem font editor

pub mod category_panel;
pub mod coordinate_panel;
pub mod edit_mode_toolbar;
pub mod editor_canvas;
pub mod glyph_anatomy_panel;
pub mod glyph_info_panel;
pub mod glyph_preview_widget;
pub mod grid_scroll_handler;
pub mod mark_color_panel;
pub mod master_toolbar;
pub mod shapes_toolbar;
pub mod size_tracker;
pub mod system_toolbar;
pub mod text_direction_toolbar;
pub mod toolbars;
pub mod transform_panel;
pub mod workspace_toolbar;

use kurbo::{Axis, Size};
use masonry::layout::{AsUnit, LenReq, Length};

/// Measure helper for leaf widgets with a fixed intrinsic size.
pub(crate) fn measure_fixed(axis: Axis, size: Size) -> Length {
    match axis {
        Axis::Horizontal => size.width.px(),
        Axis::Vertical => size.height.px(),
    }
}

/// Measure helper for widgets that fill all available space.
pub(crate) fn measure_fill(len_req: LenReq, min_px: f64) -> Length {
    match len_req {
        LenReq::FitContent(space) => space,
        LenReq::MinContent | LenReq::MaxContent => min_px.px(),
    }
}

// Re-export commonly used widget views and types
pub use category_panel::{CATEGORY_PANEL_WIDTH, GlyphCategory, category_panel};
pub use coordinate_panel::{CoordinateSelection, coordinate_panel};
pub use edit_mode_toolbar::edit_mode_toolbar_view;
pub use editor_canvas::editor_view;
pub use glyph_anatomy_panel::glyph_anatomy_panel;
pub use glyph_info_panel::{GLYPH_INFO_PANEL_WIDTH, glyph_info_panel};
pub use glyph_preview_widget::{glyph_view, multi_glyph_view};
pub use grid_scroll_handler::{GridScrollAction, NavDirection, grid_scroll_handler};
pub use mark_color_panel::mark_color_panel;
pub use master_toolbar::{create_master_infos, master_toolbar_view};
pub use shapes_toolbar::shapes_toolbar_view;
pub use size_tracker::size_tracker;
pub use system_toolbar::{SystemToolbarButton, system_toolbar_view};
pub use text_direction_toolbar::text_direction_toolbar_view;
pub use transform_panel::{TransformAction, transform_panel};
pub use workspace_toolbar::workspace_toolbar_view;

// ============================================================================
// HEADLESS RENDER TESTS
// ============================================================================
// Render each custom widget to a PNG so layout/paint regressions are
// visible without launching the app. Output dir: $RB_SHOT_DIR or
// /tmp/rb-shots. Run: cargo test render_ -- --nocapture

#[cfg(test)]
mod render_tests {
    use masonry::core::NewWidget;
    use masonry::theme::default_property_set;
    use masonry_testing::TestHarness;

    fn save(name: &str, img: image::RgbaImage) {
        let dir = std::env::var("RB_SHOT_DIR")
            .unwrap_or_else(|_| "/tmp/rb-shots".to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let path = format!("{dir}/{name}.png");
        img.save(&path).unwrap();
        println!("saved {path} ({}x{})", img.width(), img.height());
    }

    struct NopProxy;
    impl xilem::core::RawProxy for NopProxy {
        fn send_message(
            &self,
            _path: std::sync::Arc<[xilem::core::ViewId]>,
            _message: xilem::core::SendMessage,
        ) -> Result<(), xilem::core::ProxyError> {
            Ok(())
        }
        fn dyn_debug(&self) -> &dyn std::fmt::Debug {
            &"NopProxy"
        }
    }

    /// Build any WidgetView into a widget tree and render it.
    fn render_view<V>(
        view: V,
        state: &mut crate::data::AppState,
        size: (u32, u32),
    ) -> image::RgbaImage
    where
        V: xilem::WidgetView<crate::data::AppState>,
    {
        use xilem::core::View;
        let runtime = std::sync::Arc::new(
            xilem::tokio::runtime::Runtime::new().unwrap(),
        );
        let mut ctx = xilem::ViewCtx::new(
            std::sync::Arc::new(NopProxy),
            runtime,
        );
        // Wrap in sized_box so the root widget is a sized type
        // (TestHarness cannot host `dyn Widget` roots).
        let view = xilem::view::sized_box(view);
        let (pod, _view_state) = view.build(&mut ctx, state);
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            pod.new_widget,
            size,
        );
        harness.render()
    }

    fn test_state() -> crate::data::AppState {
        let mut state = crate::data::AppState::new();
        state.load_font(std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/hyper-matisse.ufo"
        )));
        state
    }

    #[test]
    fn render_full_app_rebuild() {
        use xilem::core::View;
        let mut state = test_state();
        let runtime = std::sync::Arc::new(
            xilem::tokio::runtime::Runtime::new().unwrap(),
        );
        let mut ctx = xilem::ViewCtx::new(
            std::sync::Arc::new(NopProxy),
            runtime,
        );
        let view1 = xilem::view::sized_box(crate::tabbed_view(&mut state));
        let (pod, mut view_state) = view1.build(&mut ctx, &mut state);
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            pod.new_widget,
            (1600, 1000),
        );
        save("rebuild_0_initial", harness.render());

        // Simulate what SizeChanged does on first layout, then a
        // xilem rebuild — the path the live app takes immediately.
        state.window_width = 1600.0
            - super::CATEGORY_PANEL_WIDTH
            - super::GLYPH_INFO_PANEL_WIDTH
            - 6.0 * 4.0;
        state.window_height = 1000.0;
        let view2 = xilem::view::sized_box(crate::tabbed_view(&mut state));
        harness.edit_root_widget(|widget_mut| {
            view2.rebuild(
                &view1,
                &mut view_state,
                &mut ctx,
                widget_mut,
                &mut state,
            );
        });
        save("rebuild_1_after", harness.render());
    }

    #[test]
    fn render_designspace_grid() {
        let path = std::path::PathBuf::from(
            "/Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk.designspace",
        );
        if !path.exists() {
            return;
        }
        let mut state = crate::data::AppState::new();
        state.load_font(path);
        let view = crate::tabbed_view(&mut state);
        let img = render_view(view, &mut state, (1600, 1000));
        save("designspace_grid", img);
    }

    #[test]
    fn render_with_watcher_fork() {
        use xilem::core::View;
        let mut state = test_state();
        let runtime = std::sync::Arc::new(
            xilem::tokio::runtime::Runtime::new().unwrap(),
        );
        let mut ctx = xilem::ViewCtx::new(
            std::sync::Arc::new(NopProxy),
            runtime,
        );
        let view = xilem::view::sized_box(
            crate::tabbed_view_with_watcher(&mut state),
        );
        let (pod, _vs) = view.build(&mut ctx, &mut state);
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            pod.new_widget,
            (1600, 1000),
        );
        save("watcher_fork", harness.render());
    }

    #[test]
    fn render_hidpi_scale2() {
        use xilem::core::View;
        let mut state = test_state();
        let runtime = std::sync::Arc::new(
            xilem::tokio::runtime::Runtime::new().unwrap(),
        );
        let mut ctx = xilem::ViewCtx::new(
            std::sync::Arc::new(NopProxy),
            runtime,
        );
        let view = xilem::view::sized_box(crate::tabbed_view(&mut state));
        let (pod, _vs) = view.build(&mut ctx, &mut state);
        let mut params = masonry_testing::TestHarnessParams::default();
        params.window_size = (3200, 2000).into();
        params.scale_factor = 2.0;
        let mut harness = TestHarness::create_with(
            default_property_set(),
            pod.new_widget,
            params,
        );
        save("hidpi_scale2", harness.render());
    }

    #[test]
    fn render_after_resize() {
        use xilem::core::View;
        let mut state = test_state();
        let runtime = std::sync::Arc::new(
            xilem::tokio::runtime::Runtime::new().unwrap(),
        );
        let mut ctx = xilem::ViewCtx::new(
            std::sync::Arc::new(NopProxy),
            runtime,
        );
        let view = xilem::view::sized_box(crate::tabbed_view(&mut state));
        let (pod, _vs) = view.build(&mut ctx, &mut state);
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            pod.new_widget,
            (1200, 800),
        );
        save("resize_0_before", harness.render());
        harness.process_window_event(
            masonry::core::WindowEvent::Resize((1600u32, 1000u32).into()),
        );
        save("resize_1_after", harness.render());
        harness.process_window_event(
            masonry::core::WindowEvent::Resize((900u32, 700u32).into()),
        );
        save("resize_2_smaller", harness.render());
    }

    #[test]
    fn render_full_app_root() {
        let mut state = test_state();
        let view = crate::tabbed_view(&mut state);
        let img = render_view(view, &mut state, (1600, 1000));
        save("full_app_root", img);
    }

    #[test]
    fn render_full_grid_tab() {
        let mut state = test_state();
        let view = crate::views::glyph_grid::glyph_grid_tab(&mut state);
        let img = render_view(view, &mut state, (1600, 1000));
        save("full_grid_tab", img);
    }

    #[test]
    fn render_full_editor_tab() {
        let mut state = test_state();
        state.open_editor("one".to_string());
        let view = crate::views::editor::editor_tab(&mut state);
        let img = render_view(view, &mut state, (1600, 1000));
        save("full_editor_tab", img);
    }

    #[test]
    fn render_master_toolbar() {
        let masters = vec![
            super::master_toolbar::MasterInfo {
                index: 0,
                name: "Light".into(),
                style_name: "Light".into(),
                preview_path: None,
            },
            super::master_toolbar::MasterInfo {
                index: 1,
                name: "Bold".into(),
                style_name: "Bold".into(),
                preview_path: None,
            },
        ];
        let widget = NewWidget::new(
            super::master_toolbar::MasterToolbarWidget::new(masters, 0),
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            widget,
            (400, 120),
        );
        save("master_toolbar", harness.render());
    }

    #[test]
    fn render_system_toolbar() {
        let widget = NewWidget::new(
            super::system_toolbar::SystemToolbarWidget::new(),
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            widget,
            (200, 120),
        );
        save("system_toolbar", harness.render());
    }

    #[test]
    fn render_edit_mode_toolbar() {
        let widget = NewWidget::new(
            super::edit_mode_toolbar::EditModeToolbarWidget::new(
                crate::tools::ToolId::Select,
            ),
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            widget,
            (700, 120),
        );
        save("edit_mode_toolbar", harness.render());
    }

    #[test]
    fn render_transform_panel() {
        let widget = NewWidget::new(
            super::transform_panel::TransformPanelWidget::new(true, 3),
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            widget,
            (200, 500),
        );
        save("transform_panel", harness.render());
    }

    #[test]
    fn render_mark_color_panel() {
        let widget = NewWidget::new(
            super::mark_color_panel::MarkColorPanelWidget::new(Some(2)),
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            widget,
            (300, 400),
        );
        save("mark_color_panel", harness.render());
    }

    #[test]
    fn render_category_panel() {
        let widget = NewWidget::new(
            super::category_panel::CategoryListWidget::new(
                super::category_panel::GlyphCategory::All,
            ),
        );
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            widget,
            (300, 700),
        );
        save("category_panel", harness.render());
    }
}
