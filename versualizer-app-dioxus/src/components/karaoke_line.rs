use crate::state::KaraokeState;
use dioxus::prelude::*;

/// Number of upcoming lines to show (excluding current)
const VISIBLE_NEXT_LINES: usize = 2;

/// Karaoke display component that shows current and upcoming lyrics.
/// Uses precomputed timing to drive animations locally.
#[component]
pub fn KaraokeLine(
    /// Color for text that has been sung (e.g., "#00FF00")
    sung_color: String,
    /// Color for text that hasn't been sung yet (e.g., "#FFFFFF")
    unsung_color: String,
) -> Element {
    let karaoke = use_context::<KaraokeState>();

    // Read signals
    let is_playing = *karaoke.is_playing.read();
    let current_index = *karaoke.current_index.read();
    let lyrics = karaoke.lyrics.read();

    // Get visible lines (0 previous, current + next lines)
    let visible = karaoke.visible_lines(0, VISIBLE_NEXT_LINES);

    // Container style
    let container_style = format!("--sung-color: {sung_color}; --unsung-color: {unsung_color};");

    // Play state for CSS animation
    let play_state = if is_playing { "running" } else { "paused" };

    // Determine which index in `visible` is the current line
    // Since we request 0 previous lines, current is always index 0 if it exists
    let current_visible_idx = if current_index.is_some() && !visible.is_empty() {
        Some(0)
    } else {
        None
    };

    // If no lyrics loaded, show nothing
    if lyrics.is_none() || visible.is_empty() {
        return rsx! {
            div {
                class: "karaoke-container",
                style: "{container_style}",
            }
        };
    }

    rsx! {
        div {
            class: "karaoke-container",
            style: "{container_style}",

            for (idx, line) in visible.iter().enumerate() {
                {
                    let is_current = current_visible_idx == Some(idx);
                    let opacity = if is_current { 1.0 } else { 0.5 };
                    let line_style = format!(
                        "--duration: {}ms; --play-state: {play_state}; opacity: {opacity};",
                        line.duration_ms
                    );

                    // Use line start time as key to force animation restart on line change
                    let line_key = line.start_time_ms;

                    rsx! {
                        div {
                            key: "{line_key}",
                            class: if is_current { "karaoke-line current" } else { "karaoke-line upcoming" },
                            style: "{line_style}",

                            if is_current {
                                // Current line with fill animation
                                span {
                                    class: "karaoke-background",
                                    "{line.text}"
                                }
                                span {
                                    class: "karaoke-foreground",
                                    "{line.text}"
                                }
                            } else {
                                // Upcoming lines - just show text
                                span {
                                    class: "karaoke-upcoming-text",
                                    "{line.text}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
