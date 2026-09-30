// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal macOS behind-window material, for Xilem issue #1852.
//! Run with `cargo run --profile fast --locked --example macos_vibrancy`.

#[cfg(target_os = "macos")]
mod macos {
    use masonry::layout::{Dim, Length};
    use masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceName, NSApplication,
        NSAutoresizingMaskOptions, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindowButton, NSWindowOrderingMode,
    };
    use xilem::core::fork;
    use xilem::style::Style;
    use xilem::view::{flex_col, label, sized_box, task, zstack};
    use xilem::{Color, EventLoop, WindowId, WindowOptionsExtMacOS, Xilem};

    struct State {
        running: bool,
        effect: Option<Retained<NSVisualEffectView>>,
    }

    impl xilem::AppState for State {
        fn keep_running(&self) -> bool {
            self.running
        }
    }

    fn install_effect(state: &mut State) {
        let mtm = MainThreadMarker::new().expect("Xilem runs on the main thread");
        let app = NSApplication::sharedApplication(mtm);
        let Some(window) = app.mainWindow().or_else(|| app.keyWindow()) else {
            return;
        };
        let Some(content) = window.contentView() else {
            return;
        };
        let Some(close) = window.standardWindowButton(NSWindowButton::CloseButton) else {
            return;
        };
        let Some(parent) = content.ancestorSharedWithView(&close) else {
            return;
        };
        let effect = NSVisualEffectView::initWithFrame(mtm.alloc(), content.frame());
        effect.setMaterial(NSVisualEffectMaterial::Menu);
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::Active);
        effect.setAppearance(
            NSAppearance::appearanceNamed(&NSAppearanceName::from_str(
                "NSAppearanceNameVibrantDark",
            ))
            .as_deref(),
        );
        effect.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        // Keep winit's content view intact: the native material is its sibling.
        parent.addSubview_positioned_relativeTo(
            &effect,
            NSWindowOrderingMode::Below,
            Some(&content),
        );
        state.effect = Some(effect);
    }

    pub(super) fn run() -> Result<(), winit::error::EventLoopError> {
        let id = WindowId::next();
        Xilem::new(
            State {
                running: true,
                effect: None,
            },
            move |state| {
                let panel = sized_box(zstack((label("Rounded panel")
                    .text_size(20.0)
                    .color(Color::from_rgb8(48, 48, 48)),)))
                .fixed_width(Length::px(320.0))
                .fixed_height(Length::px(160.0))
                .background_color(Color::from_rgb8(165, 165, 165))
                .border_color(Color::from_rgb8(48, 48, 48))
                .border_width(Length::px(1.0))
                .corner_radius(Length::px(16.0));
                let view = fork(
                    flex_col((label("Hello, Xilem").text_size(24.0), panel))
                        .main_axis_alignment(MainAxisAlignment::Center)
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .dims((Dim::Stretch, Dim::Stretch))
                        .gap(Length::px(24.0)),
                    state.effect.is_none().then(|| {
                        task(
                            |proxy, _: &mut State| async move {
                                loop {
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                    if proxy.message(()).is_err() {
                                        break;
                                    }
                                }
                            },
                            |state: &mut State, ()| install_effect(state),
                        )
                    }),
                );
                std::iter::once(
                    xilem::window(id, "Xilem — macOS vibrancy", view)
                        .with_base_color(Color::TRANSPARENT)
                        .with_options(|o| {
                            o.with_transparent(true)
                                .with_titlebar_transparent(true)
                                .with_fullsize_content_view(true)
                                .with_initial_inner_size(winit::dpi::LogicalSize::new(640.0, 420.0))
                                .on_close(|state: &mut State| state.running = false)
                        }),
                )
            },
        )
        .with_default_base_color(Color::TRANSPARENT)
        .run_in(EventLoop::with_user_event())
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    macos::run().expect("run the macOS vibrancy example");
    #[cfg(not(target_os = "macos"))]
    eprintln!("This diagnostic example requires macOS.");
}
