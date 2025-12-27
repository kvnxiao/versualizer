use crate::state::{KaraokeDisplayConfig, KaraokeState, PrecomputedLyrics, INTRO_LINE_INDEX};
use dioxus::prelude::*;
use dioxus_motion::prelude::*;

/// Buffer lines for smooth animation (not user-configurable).
/// These extra lines are rendered outside the visible area to enable
/// smooth fade in/out transitions.
const BUFFER_LINES_BEFORE: usize = 1;
const BUFFER_LINES_AFTER: usize = 1;

/// Layout constants that must match the CSS variables.
/// These are used to calculate transform positions.
const BASE_FONT_SIZE_PX: f32 = 32.0;
const LINE_HEIGHT_MULTIPLIER: f32 = 1.5;
const LINE_GAP_PX: f32 = 8.0;

/// Calculate the total height of one line slot (font height + gap).
const fn calculate_line_slot_height() -> f32 {
    BASE_FONT_SIZE_PX * LINE_HEIGHT_MULTIPLIER + LINE_GAP_PX
}

/// Karaoke display component that shows current and upcoming lyrics
/// with smooth animations powered by dioxus-motion.
#[component]
pub fn KaraokeLine(
    /// Color for text that has been sung (e.g., "#00FF00")
    sung_color: String,
    /// Color for text that hasn't been sung yet (e.g., "#FFFFFF")
    unsung_color: String,
) -> Element {
    let karaoke = use_context::<KaraokeState>();
    let config = use_context::<KaraokeDisplayConfig>();

    // Read signals
    let is_playing = *karaoke.is_playing.read();
    let current_index = *karaoke.current_index.read(); // i32: -1 = intro, 0+ = line index
    let lyrics = karaoke.lyrics.read();

    // Calculate how many lines to request (visible + buffer)
    let visible_count = config.max_lines;
    let lines_after = visible_count.saturating_sub(1) + BUFFER_LINES_AFTER;

    // Get visible lines with buffer
    let visible = karaoke.visible_lines(BUFFER_LINES_BEFORE, lines_after);

    // Calculate container height based on visible lines
    let line_slot_height = calculate_line_slot_height();
    // Safe cast: visible_count is clamped to 1-3 in config, well within f32 range
    #[allow(clippy::cast_precision_loss)]
    let container_height = line_slot_height * (visible_count as f32);

    // Animated scroll offset - represents the current line index as a float
    // -1.0 for intro, 0.0+ for actual lines
    // Safe cast: INTRO_LINE_INDEX is -1, which is exactly representable in f32
    #[allow(clippy::cast_precision_loss)]
    let mut scroll_offset = use_motion(INTRO_LINE_INDEX as f32);

    // Keep Signal reference for use in effect (must read INSIDE effect for reactivity)
    let current_index_signal = karaoke.current_index;

    // Animate scroll offset when current line changes
    use_effect(move || {
        // Read signal INSIDE effect - creates reactive dependency so effect re-runs
        let target_offset = *current_index_signal.read();

        #[allow(clippy::cast_precision_loss)]
        let target = target_offset as f32;

        // Use spring animation for smooth, natural scrolling
        scroll_offset.animate_to(
            target,
            AnimationConfig::new(AnimationMode::Spring(Spring {
                stiffness: 180.0,
                damping: 20.0,
                mass: 1.0,
                ..Default::default()
            })),
        );
    });

    // Container style with dynamic height and CSS variables
    let container_style = format!(
        "--sung-color: {sung_color}; --unsung-color: {unsung_color}; \
         --transition-duration: {}ms; --transition-easing: {}; \
         height: {container_height}px;",
        config.transition_ms, config.easing
    );

    // Play state for CSS animation
    let play_state = if is_playing { "running" } else { "paused" };

    // Check if there's an intro (lyrics exist and have intro duration > 0)
    let has_intro = lyrics
        .as_ref()
        .is_some_and(PrecomputedLyrics::has_intro);

    // Determine which index in `visible` array is the current line.
    // When current_index < 0 (intro), the first visible line is the intro line at index 0.
    // When current_index >= 0, we need to account for buffer lines before.
    let current_visible_idx: Option<usize> = if current_index < 0 {
        // In intro: if there's an intro line, it's at visible[0]
        if has_intro { Some(0) } else { None }
    } else {
        // On an actual line - calculate position in visible array
        #[allow(clippy::cast_sign_loss)]
        let idx = current_index as usize;
        let actual_buffer_before = idx.min(BUFFER_LINES_BEFORE);
        if visible.len() > actual_buffer_before {
            Some(actual_buffer_before)
        } else {
            None
        }
    };

    // If no lyrics loaded, show empty container
    if lyrics.is_none() || visible.is_empty() {
        return rsx! {
            div {
                class: "karaoke-container",
                style: "{container_style}",
            }
        };
    }

    // Get the current animated scroll value
    let animated_offset = scroll_offset.get_value();

    // Calculate where the visible array starts in absolute line index terms.
    // In intro mode (current_index < 0), visible starts at -1 (intro line).
    // Otherwise, it starts at max(0, current_index - BUFFER_LINES_BEFORE).
    let visible_start_idx: i32 = if current_index < 0 {
        INTRO_LINE_INDEX // -1
    } else {
        // Safe: BUFFER_LINES_BEFORE is a small constant (1)
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let buffer = BUFFER_LINES_BEFORE as i32;
        (current_index - buffer).max(0)
    };

    rsx! {
        div {
            class: "karaoke-container",
            style: "{container_style}",

            for (idx, line) in visible.iter().enumerate() {
                {
                    // Calculate the absolute line index for this visible line
                    // visible[0] corresponds to visible_start_idx in absolute terms
                    // Safe: idx is a small index into visible array (typically < 10 elements)
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    let line_absolute_idx = visible_start_idx + (idx as i32);

                    // Calculate vertical offset using animated scroll value
                    // Each line's position is: (line_absolute_idx - animated_offset) * line_slot_height
                    #[allow(clippy::cast_precision_loss)]
                    let y_offset = ((line_absolute_idx as f32) - animated_offset) * line_slot_height;

                    // Determine if this line is currently the "current" line
                    // based on the animated offset (with some threshold)
                    #[allow(clippy::cast_precision_loss)]
                    let distance_from_current = (line_absolute_idx as f32) - animated_offset;
                    let is_current = distance_from_current.abs() < 0.5
                        && current_visible_idx == Some(idx);

                    // Calculate scale based on distance from current
                    // Smooth interpolation between current and upcoming scale
                    let scale = if distance_from_current.abs() < 1.0 {
                        // Interpolate between current and upcoming scale using mul_add for accuracy
                        let t = distance_from_current.abs();
                        config
                            .current_line_scale
                            .mul_add(1.0 - t, config.upcoming_line_scale * t)
                    } else {
                        config.upcoming_line_scale
                    };

                    // Calculate opacity based on position
                    // Buffer zone: opacity 0
                    // Visible zone: current = 1.0, upcoming = 0.5
                    #[allow(clippy::cast_precision_loss)]
                    let visible_count_f32 = visible_count as f32;
                    let opacity = if distance_from_current < 0.0 {
                        // Above current (buffer zone above)
                        0.0_f32.max(1.0 + distance_from_current)
                    } else if distance_from_current >= visible_count_f32 {
                        // Below visible area (buffer zone below)
                        0.0_f32.max(1.0 - (distance_from_current - visible_count_f32 + 1.0))
                    } else if distance_from_current < 0.5 {
                        // Current line zone
                        1.0
                    } else {
                        // Upcoming lines
                        0.5
                    };

                    let line_class = if is_current {
                        "karaoke-line current"
                    } else {
                        "karaoke-line upcoming"
                    };

                    // Inline style with animated transform and opacity
                    let line_style = format!(
                        "transform: translateY({y_offset}px) scale({scale}); \
                         opacity: {opacity}; \
                         --duration: {}ms; --play-state: {play_state};",
                        line.duration_ms
                    );

                    // Use absolute line index as key for stable DOM elements
                    let line_key = format!("line-{line_absolute_idx}");

                    // For karaoke fill animation, use start_time to force restart
                    let animation_key = line.start_time_ms;

                    rsx! {
                        div {
                            key: "{line_key}",
                            class: "{line_class}",
                            style: "{line_style}",

                            if is_current {
                                // Current line with karaoke fill animation
                                // Wrap in a keyed div to restart animation on line change
                                div {
                                    key: "{animation_key}",
                                    class: "karaoke-text-wrapper",
                                    span {
                                        class: "karaoke-background",
                                        "{line.text}"
                                    }
                                    span {
                                        class: "karaoke-foreground",
                                        "{line.text}"
                                    }
                                }
                            } else {
                                // Upcoming/buffer lines - static text
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
