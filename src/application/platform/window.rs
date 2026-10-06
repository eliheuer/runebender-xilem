// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! macOS window state and optional native backdrop, outside the font engine.

use crate::application::view::theme::BackdropStyle;
use crate::application::workspace::AppState;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceName, NSApplication,
    NSAutoresizingMaskOptions, NSColor, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowButton, NSWindowOrderingMode,
    NSWindowStyleMask, NSWorkspace,
};
use runebender::ui::theme::{BlurAppearance, BlurMaterial};
use std::cell::RefCell;

struct Backdrop {
    window: Retained<NSWindow>,
    content: Retained<NSView>,
    effect: Retained<NSVisualEffectView>,
}

thread_local! {
    static BACKDROP: RefCell<Option<Backdrop>> = const { RefCell::new(None) };
}

/// True only after the native effect is installed; headless rendering remains flat.
pub(crate) fn backdrop_active() -> bool {
    BACKDROP.with(|state| {
        state
            .borrow()
            .as_ref()
            .is_some_and(|backdrop| backdrop.effect.window().is_some())
    })
}

/// Retry setup after winit has shown the window, then stop requesting redraws.
/// The first Xilem build and layout can both precede `AppKit`'s window registration.
pub(crate) fn with_backdrop<V: xilem::WidgetView<AppState>>(
    view: V,
    enabled: bool,
) -> impl xilem::WidgetView<AppState> + use<V> {
    use xilem::core::{MessageProxy, fork};
    use xilem::view::task;

    let pending = enabled
        && !backdrop_active()
        && !NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency();
    fork(
        view,
        pending.then(|| {
            task(
                |proxy: MessageProxy<()>, _: &mut AppState| async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        if proxy.message(()).is_err() {
                            return;
                        }
                    }
                },
                |app: &mut AppState, ()| set_backdrop(app.palette.backdrop_style()),
            )
        }),
    )
}

/// Add an `AppKit` backdrop behind the GPU view, preserving its input and drawing view.
///
/// Winit casts the window's content view to its own class, so it must never be replaced.
/// The effect is a sibling below that view; removing it makes theme changes reversible.
/// Desktop testing on 2026-09-30 confirmed wallpaper blur in the full editor with transparent
/// startup and zero application tint (user capture 1852-005).
/// A minimal Xilem label and rounded box reproduced bright, jagged edges over the backdrop.
/// Updating the isolated experiment to Vello 0.10 and wgpu 30.0.1 did not resolve those edges.
/// Backporting gfx-rs/wgpu#9922 to that experiment made the demo and editor text smooth,
/// as confirmed by the user on-screen. The pinned Xilem fork now applies the equivalent
/// premultiplying Metal blit to the supported renderer versions.
/// The correction advertises `PreMultiplied` on Metal, enabling Masonry's premultiplying blit
/// to match Core Animation's compositing instead of presenting straight-alpha pixels.
/// Panel contents use a rounded subtree clip instead of opaque ground-colored corner masks.
/// The user accepted the rebuilt native result after that clipping change.
/// Earlier application tint opacity 0.8 and 0.65 produced washed-out gray.
/// Dark and Gray now enable the backdrop with independent 25% gray overlays.
/// Xilem dependency/workaround report: <https://github.com/linebender/xilem/issues/1852>.
pub(crate) fn set_backdrop(style: BackdropStyle) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main_thread);
    // During the first layout the window can exist before AppKit declares it
    // the main window. Do not wait for a later click or resize to install blur.
    let window = application
        .mainWindow()
        .or_else(|| application.keyWindow())
        .or_else(|| {
            application
                .windows()
                .iter()
                .find(|window| window.isVisible())
        });
    let enabled = style.enabled
        && !NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency();
    BACKDROP.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(backdrop) = state.as_ref()
            && (!enabled || window.as_ref() != Some(&backdrop.window))
        {
            backdrop.effect.removeFromSuperview();
            // The GPU view paints the whole window again, so the window can be solid.
            backdrop.window.setOpaque(true);
            *state = None;
        }
        if !enabled {
            return;
        }
        if let Some(backdrop) = state.as_ref() {
            apply_backdrop_style(&backdrop.effect, style);
            // Full-screen transitions can move the content into a new native parent.
            if let Some(parent) = backdrop_parent(&backdrop.window, &backdrop.content) {
                if !parent.subviews().containsObject(&backdrop.effect) {
                    backdrop.effect.removeFromSuperview();
                    parent.addSubview_positioned_relativeTo(
                        &backdrop.effect,
                        NSWindowOrderingMode::Below,
                        Some(&backdrop.content),
                    );
                }
                backdrop.effect.setFrame(backdrop.content.frame());
            }
            return;
        }
        let Some(window) = window else {
            return;
        };
        let Some(content) = window.contentView() else {
            return;
        };
        let Some(parent) = backdrop_parent(&window, &content) else {
            return;
        };
        // A window created solid, with the theme's blur off, has to go clear for the effect
        // to show through; this is what winit's transparent option does at creation.
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        let effect = NSVisualEffectView::initWithFrame(main_thread.alloc(), content.frame());
        apply_backdrop_style(&effect, style);
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::FollowsWindowActiveState);
        effect.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        parent.addSubview_positioned_relativeTo(
            &effect,
            NSWindowOrderingMode::Below,
            Some(&content),
        );
        if std::env::var_os("RUNEBENDER_DEBUG_BACKDROP").is_some() {
            eprintln!(
                "macOS backdrop installed: opaque={}, content opaque={}, key={}, state={:?}, effect frame={:?}",
                window.isOpaque(),
                content.isOpaque(),
                window.isKeyWindow(),
                effect.state(),
                effect.frame(),
            );
        }
        *state = Some(Backdrop {
            window,
            content,
            effect,
        });
    });
}

/// Apply the theme's material and appearance, also when a theme switch changes them.
/// An automatic appearance follows the window ground, so system light mode does not put a
/// pale material beneath a dark application theme.
fn apply_backdrop_style(effect: &NSVisualEffectView, style: BackdropStyle) {
    effect.setMaterial(match style.material {
        BlurMaterial::Menu => NSVisualEffectMaterial::Menu,
        BlurMaterial::Popover => NSVisualEffectMaterial::Popover,
        BlurMaterial::Sidebar => NSVisualEffectMaterial::Sidebar,
        BlurMaterial::HeaderView => NSVisualEffectMaterial::HeaderView,
        BlurMaterial::HudWindow => NSVisualEffectMaterial::HUDWindow,
        BlurMaterial::FullScreenUi => NSVisualEffectMaterial::FullScreenUI,
        BlurMaterial::UnderWindowBackground => NSVisualEffectMaterial::UnderWindowBackground,
        BlurMaterial::WindowBackground => NSVisualEffectMaterial::WindowBackground,
    });
    let dark = match style.appearance {
        BlurAppearance::Dark => true,
        BlurAppearance::Light => false,
        BlurAppearance::Auto => {
            crate::application::view::theme::window_theme_for_color(style.ground)
                == winit::window::Theme::Dark
        }
    };
    let name = if dark {
        "NSAppearanceNameVibrantDark"
    } else {
        "NSAppearanceNameVibrantLight"
    };
    effect
        .setAppearance(NSAppearance::appearanceNamed(&NSAppearanceName::from_str(name)).as_deref());
}

/// Obtain the common native frame through `AppKit`'s safe ancestry API.
/// The content and close button are both retained descendants of the same window.
fn backdrop_parent(window: &NSWindow, content: &NSView) -> Option<Retained<NSView>> {
    let close = window.standardWindowButton(NSWindowButton::CloseButton)?;
    content.ancestorSharedWithView(&close)
}

/// Read `AppKit`'s full-screen flag, rather than infer it from window dimensions.
pub(crate) fn is_fullscreen() -> bool {
    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };
    NSApplication::sharedApplication(main_thread)
        .mainWindow()
        .is_some_and(|window| window.styleMask().contains(NSWindowStyleMask::FullScreen))
}
