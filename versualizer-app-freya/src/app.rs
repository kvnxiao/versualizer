use crate::state::{AppChannel, AppState};
use freya::animation::{use_animation_with_dependencies, AnimNum, Function, OnChange, OnCreation};
use freya::prelude::*;
use freya_radio::prelude::*;
use std::borrow::Cow;

/// Type alias for RGB color used in karaoke lines
pub type KaraokeColor = (u8, u8, u8);

/// Parse a hex color string to RGB tuple
fn parse_hex_color(hex: &str) -> KaraokeColor {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        (r, g, b)
    } else {
        (255, 255, 255) // Default to white
    }
}

/// Karaoke line component props - holds configuration only, state comes from signals
#[derive(PartialEq, Clone)]
pub struct KaraokeLineComponent {
    pub sung_color: KaraokeColor,
    pub unsung_color: KaraokeColor,
    pub font_size: f32,
    pub font_family: Cow<'static, str>,
}

impl Render for KaraokeLineComponent {
    fn render(&self) -> impl IntoElement {
        // Subscribe to radio channels for reactive updates
        let radio_lyrics = use_radio::<AppState, AppChannel>(AppChannel::Lyrics);
        let radio_line = use_radio::<AppState, AppChannel>(AppChannel::LineChange);

        // Read current line data from signals
        let line_duration_ms = radio_line.read().line_duration_ms;
        let current_line_idx = radio_line.read().current_line_index;

        // Get current line text from lyrics
        let line_text = {
            let state = radio_lyrics.read();
            state
                .lyrics
                .as_ref()
                .and_then(|lyrics| {
                    current_line_idx.and_then(|idx| lyrics.lines.get(idx).map(|l| l.text.clone()))
                })
                .unwrap_or_default()
        };

        // Animation restarts when duration or text changes
        let deps = (line_duration_ms, line_text.clone());
        let animation = use_animation_with_dependencies(&deps, move |conf, (duration, _text)| {
            conf.on_change(OnChange::Rerun);
            conf.on_creation(OnCreation::Run);
            AnimNum::new(0.0, 100.0)
                .time((*duration).max(100))
                .function(Function::Linear)
        });
        let progress = animation.get().value();

        // Clone text for ownership - animated_karaoke_line clones internally anyway
        let sung_color = self.sung_color;
        let unsung_color = self.unsung_color;
        let font_size = self.font_size;
        let font_family = self.font_family.clone();
        let progress_percent = Size::percent(progress);

        // Inline the karaoke line rendering to avoid lifetime issues
        rect()
            .horizontal()
            .center()
            .width(Size::Fill)
            .height(Size::Fill)
            // Background layer (unsung text, full width)
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(Size::Fill)
                    .height(Size::px(font_size * 2f32))
                    .child(
                        rect()
                            .color(unsung_color)
                            .font_size(font_size)
                            .font_family(font_family.clone())
                            .child(line_text.clone()),
                    ),
            )
            // Foreground layer (sung text, clipped to progress width)
            .child(
                rect()
                    .position(Position::new_absolute().left(0.0).top(0.0))
                    .width(progress_percent)
                    .height(Size::px(font_size * 2f32))
                    .overflow(Overflow::Clip)
                    .text_overflow(TextOverflow::Clip)
                    .child(
                        rect()
                            .color(sung_color)
                            .font_size(font_size)
                            .font_family(font_family)
                            .child(line_text),
                    ),
            )
    }
}

/// Main application component struct - holds the `RadioStation` from main.rs
pub struct App {
    pub radio_station: RadioStation<AppState, AppChannel>,
}

impl Render for App {
    fn render(&self) -> impl IntoElement {
        // Share the radio station from main.rs with child components
        use_share_radio(move || self.radio_station);

        // Configuration (use defaults for now)
        let sung_color = parse_hex_color("#00FF00");
        let unsung_color = parse_hex_color("#FFFFFF");
        let font_size = 36.0f32;
        let font_family: Cow<'static, str> = Cow::Borrowed("Arial");

        // Single persistent KaraokeLineComponent that reads state via signals
        // Animation restarts automatically when line duration/text changes
        let karaoke = KaraokeLineComponent {
            sung_color,
            unsung_color,
            font_size,
            font_family,
        };

        rect()
            .width(Size::fill())
            .height(Size::fill())
            .center()
            .cross_align(Alignment::End)
            .background(Color::from_argb(50, 0, 0, 0)) // rgba(0,0,0,0.5)
            .child(karaoke)
    }
}
