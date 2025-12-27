use crate::components::KaraokeLine;
use dioxus::desktop::use_window;
use dioxus::prelude::*;

/// Root application component.
/// Renders a transparent container with the karaoke line display.
#[component]
pub fn App() -> Element {
    let window = use_window();

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
