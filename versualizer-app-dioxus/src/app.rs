use crate::components::KaraokeLine;
use crate::theme_watcher::use_theme_watcher;
use crate::window_resize::use_window_auto_resize;
use crate::window_state::WindowState;
use dioxus::desktop::tao::event::{Event as WryEvent, WindowEvent};
use dioxus::desktop::{use_window, use_wry_event_handler};
use dioxus::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Root application component.
/// Renders a transparent container with the karaoke line display.
#[component]
pub fn App() -> Element {
    let window = use_window();
    let cancel_token: CancellationToken = use_context();

    // Get reactive CSS content from theme watcher
    // This watches ~/.config/versualizer/theme.css for changes and hot-reloads
    let css_content = use_theme_watcher(cancel_token.clone());

    // Auto-resize window when CSS changes affect content dimensions
    use_window_auto_resize(css_content);

    // Handle window close event (triggered by X button)
    // Save window position before closing
    let window_for_close = window.clone();
    let cancel_token_for_wry = cancel_token.clone();
    use_wry_event_handler(move |event, _| {
        if let WryEvent::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            info!("Window close requested, shutting down gracefully...");

            // Save window position before closing
            if let Ok(position) = window_for_close.outer_position() {
                let state = WindowState {
                    x: position.x,
                    y: position.y,
                };
                state.save();
            }

            cancel_token_for_wry.cancel();
        }
    });

    // Poll for Ctrl+C signal and close window when received
    use_future(move || {
        let cancel_token = cancel_token.clone();
        let window = window.clone();
        async move {
            // Wait for cancellation (triggered by Ctrl+C handler in main.rs)
            cancel_token.cancelled().await;
            info!("Cancellation detected, closing window...");
            window.close();
        }
    });

    #[cfg(target_os = "macos")]
    return rsx! {
        // Dynamic style element - re-renders when css_content signal changes
        style { dangerous_inner_html: "{css_content}" }

        div {
            class: "app",

            KaraokeLine {}
        }
    };

    #[cfg(not(target_os = "macos"))]
    {
        let window_for_drag = window.clone();
        let on_mouse_down = move |_: MouseEvent| {
            let _ = window_for_drag.drag_window();
        };
        return rsx! {
            // Dynamic style element - re-renders when css_content signal changes
            style { dangerous_inner_html: "{css_content}" }

            div {
                class: "app",
                onmousedown: on_mouse_down,

                KaraokeLine {}
            }
        };
    }
}
