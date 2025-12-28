use crate::components::KaraokeLine;
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

    // Handle window close event (triggered by X button)
    let cancel_token_for_wry = cancel_token.clone();
    use_wry_event_handler(move |event, _| {
        if let WryEvent::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            info!("Window close requested, shutting down gracefully...");
            cancel_token_for_wry.cancel();
        }
    });

    // Poll for Ctrl+C signal and close window when received
    let window_for_ctrlc = window.clone();
    use_future(move || {
        let cancel_token = cancel_token.clone();
        let window = window_for_ctrlc.clone();
        async move {
            // Wait for cancellation (triggered by Ctrl+C handler in main.rs)
            cancel_token.cancelled().await;
            info!("Cancellation detected, closing window...");
            window.close();
        }
    });

    // Handle mouse down to start window drag (for borderless window)
    let on_mouse_down = move |_: MouseEvent| {
        let _ = window.drag_window();
    };

    rsx! {
        div {
            class: "container",
            onmousedown: on_mouse_down,

            KaraokeLine {
                sung_color: "#00FF00".to_string(),
                unsung_color: "#FFFFFF".to_string(),
            }
        }
    }
}
