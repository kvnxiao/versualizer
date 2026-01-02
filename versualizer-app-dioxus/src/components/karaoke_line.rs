use crate::state::{INTRO_LINE_INDEX, KaraokeState, PrecomputedLyrics};
use dioxus::prelude::*;
use dioxus_motion::prelude::*;
use versualizer_core::UiConfig;

/// Buffer lines for smooth animation (not user-configurable).
/// These extra lines are rendered outside the visible area to enable
/// smooth fade in/out transitions.
const BUFFER_LINES_BEFORE: usize = 1;
const BUFFER_LINES_AFTER: usize = 1;

/// Karaoke display component that shows current and upcoming lyrics
/// with smooth animations powered by dioxus-motion.
///
/// Colors are configured via CSS variables in theme.css:
/// - `--sung-color`: Color for sung text (use rgba for transparency)
/// - `--unsung-color`: Color for unsung text (use rgba for transparency)
#[component]
pub fn KaraokeLine() -> Element {
    let karaoke = use_context::<KaraokeState>();
    let config = use_context::<UiConfig>();

    // Read signals
    let is_playing = *karaoke.is_playing.read();
    let current_index = *karaoke.current_index.read(); // i32: -1 = intro, 0+ = line index
    let lyrics = karaoke.lyrics.read();
    let animation_sync_position_ms = *karaoke.animation_sync_position_ms.read();

    // Calculate how many lines to request (visible + buffer)
    let visible_count = config.layout.max_lines;
    let lines_after = visible_count.saturating_sub(1) + BUFFER_LINES_AFTER;

    // Get visible lines with buffer
    let visible = karaoke.visible_lines(BUFFER_LINES_BEFORE, lines_after);

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

    // Set CSS variables from config (all calculations done in CSS)
    let container_style = format!("--max-lines: {visible_count};");

    // Play state for CSS animation
    let play_state = if is_playing { "running" } else { "paused" };

    // Check if there's an intro (lyrics exist and have intro duration > 0)
    let has_intro = lyrics.as_ref().is_some_and(PrecomputedLyrics::has_intro);

    // Determine which index in `visible` array is the current line.
    // When current_index < 0 (intro), the first visible line is the intro line at index 0.
    // When current_index >= 0, we need to account for buffer lines before.
    let current_visible_idx: Option<usize> = if current_index < 0 {
        // In intro: if there's an intro line, it's at visible[0]
        if has_intro { Some(0) } else { None }
    } else {
        // On an actual line - calculate position in visible array
        // current_index >= 0 is guaranteed by the outer if condition
        let idx = usize::try_from(current_index).unwrap_or(0);
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
                class: "lines",
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
        #[allow(clippy::cast_possible_wrap)]
        let buffer = i32::try_from(BUFFER_LINES_BEFORE).unwrap_or(i32::MAX);
        (current_index - buffer).max(0)
    };

    rsx! {
        div {
            class: "lines",
            style: "{container_style}",

            for (idx, line) in visible.iter().enumerate() {
                {
                    // Calculate the absolute line index for this visible line
                    // Safe: idx is a small index into visible array (typically < 10 elements)
                    #[allow(clippy::cast_possible_wrap)]
                    let line_absolute_idx = visible_start_idx + i32::try_from(idx).unwrap_or(i32::MAX);

                    // Distance from current line (used by CSS for scale and opacity calculations)
                    #[allow(clippy::cast_precision_loss)]
                    let distance = (line_absolute_idx as f32) - animated_offset;

                    // Determine if this line is the "current" line
                    let is_current = distance.abs() < 0.5 && current_visible_idx == Some(idx);

                    let line_class = if is_current {
                        "karaoke-line current"
                    } else {
                        "karaoke-line upcoming"
                    };

                    // Pass raw values to CSS - all transform/opacity calculations done in CSS
                    let line_duration_ms = line.duration_ms;
                    let line_style = format!(
                        "--line-index: {line_absolute_idx}; \
                         --scroll-offset: {animated_offset}; \
                         --distance: {distance}; \
                         --duration: {line_duration_ms}ms; \
                         --play-state: {play_state};",
                    );

                    // Use absolute line index as key for stable DOM elements
                    let line_key = format!("line-{line_absolute_idx}");

                    // For karaoke fill animation, use composite key to force restart on:
                    // 1. Line change (start_time changes)
                    // 2. Seek/sync within same line (animation_sync_position_ms changes)
                    let animation_key = format!("{}-{}", line.start_time_ms, animation_sync_position_ms);

                    // Calculate animation offset for seek support (negative delay starts animation partway)
                    // Only apply offset if we've synced into this line
                    let animation_delay_ms: i64 = if animation_sync_position_ms >= line.start_time_ms
                        && animation_sync_position_ms < line.start_time_ms.saturating_add(line.duration_ms)
                    {
                        // We're syncing within this line - calculate offset
                        let offset = animation_sync_position_ms.saturating_sub(line.start_time_ms);
                        // Negative delay to start animation partway through
                        // Safe: offset is always <= duration which fits in i64
                        #[allow(clippy::cast_possible_wrap)]
                        let neg_offset = -(offset as i64);
                        neg_offset
                    } else {
                        0
                    };

                    rsx! {
                        div {
                            key: "{line_key}",
                            class: "{line_class}",
                            style: "{line_style}",

                            if is_current {
                                // Current line with karaoke fill animation
                                // Wrap in a keyed div to restart animation on line change or seek
                                div {
                                    key: "{animation_key}",
                                    class: "current-line-wrapper",
                                    style: "--animation-delay: {animation_delay_ms}ms;",
                                    span {
                                        class: "current-line-unsung",
                                        "{line.text}"
                                    }
                                    span {
                                        class: "current-line-sung",
                                        "{line.text}"
                                    }
                                }
                            } else {
                                // Upcoming/buffer lines - static text
                                span {
                                    class: "upcoming-line",
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
