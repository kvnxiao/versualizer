use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub spotify: SpotifyConfig,
    pub lyrics: LyricsConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub error_handling: ErrorHandlingConfig,
    #[serde(default)]
    pub window: WindowBehaviorConfig,
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyConfig {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "default_redirect_uri")]
    pub oauth_redirect_uri: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// Optional: For unofficial Spotify lyrics API (use at your own risk)
    pub sp_dc: Option<String>,
}

fn default_redirect_uri() -> String {
    "http://127.0.0.1:8888/callback".into()
}

const fn default_poll_interval() -> u64 {
    1000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LyricsConfig {
    /// Provider priority: providers are tried in order
    #[serde(default = "default_providers")]
    pub providers: Vec<LyricsProviderType>,
    #[serde(default = "default_true")]
    pub cache_enabled: bool,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_days: u32,
}

fn default_providers() -> Vec<LyricsProviderType> {
    vec![LyricsProviderType::Lrclib]
}

const fn default_true() -> bool {
    true
}

const fn default_cache_ttl() -> u32 {
    30
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsProviderType {
    Lrclib,
    SpotifyLyrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_fill_mode")]
    pub fill_mode: FillMode,
    #[serde(default = "default_sung_color")]
    pub sung_color: String,
    #[serde(default = "default_unsung_color")]
    pub unsung_color: String,
    #[serde(default = "default_background_color")]
    pub background_color: String,
    #[serde(default)]
    pub font: FontConfig,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub animation: AnimationConfig,
    #[serde(default)]
    pub window: WindowConfig,
}

const fn default_fill_mode() -> FillMode {
    FillMode::Gradient
}

fn default_sung_color() -> String {
    "#00FF00".to_string()
}

fn default_unsung_color() -> String {
    "#FFFFFF".to_string()
}

fn default_background_color() -> String {
    "#00000000".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FillMode {
    Character,
    #[default]
    Gradient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default = "default_font_size")]
    pub size: f32,
    #[serde(default = "default_font_weight")]
    pub weight: String,
    #[serde(default)]
    pub fallbacks: Vec<String>,
}

fn default_font_family() -> String {
    "Arial".to_string()
}

const fn default_font_size() -> f32 {
    24.0
}

fn default_font_weight() -> String {
    "normal".to_string()
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            size: default_font_size(),
            weight: default_font_weight(),
            fallbacks: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    #[serde(default = "default_alignment")]
    pub alignment: String,
    #[serde(default = "default_vertical_position")]
    pub vertical_position: String,
    #[serde(default = "default_margin")]
    pub margin_horizontal: f32,
    #[serde(default = "default_margin_vertical")]
    pub margin_vertical: f32,
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    #[serde(default = "default_current_line_scale")]
    pub current_line_scale: f32,
}

fn default_alignment() -> String {
    "center".to_string()
}

fn default_vertical_position() -> String {
    "bottom".to_string()
}

const fn default_margin() -> f32 {
    20.0
}

const fn default_margin_vertical() -> f32 {
    10.0
}

const fn default_max_lines() -> usize {
    3
}

const fn default_current_line_scale() -> f32 {
    1.2
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            alignment: default_alignment(),
            vertical_position: default_vertical_position(),
            margin_horizontal: default_margin(),
            margin_vertical: default_margin_vertical(),
            max_lines: default_max_lines(),
            current_line_scale: default_current_line_scale(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationConfig {
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u32,
    #[serde(default = "default_transition_ms")]
    pub transition_ms: u32,
    #[serde(default = "default_easing")]
    pub easing: String,
}

const fn default_frame_rate() -> u32 {
    60
}

const fn default_transition_ms() -> u32 {
    200
}

fn default_easing() -> String {
    "ease_in_out".to_string()
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            frame_rate: default_frame_rate(),
            transition_ms: default_transition_ms(),
            easing: default_easing(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_window_width")]
    pub width: u32,
    #[serde(default = "default_window_height")]
    pub height: u32,
    #[serde(default = "default_window_pos")]
    pub start_x: i32,
    #[serde(default = "default_window_pos")]
    pub start_y: i32,
}

const fn default_window_width() -> u32 {
    800
}

const fn default_window_height() -> u32 {
    200
}

const fn default_window_pos() -> i32 {
    -1 // centered
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: default_window_width(),
            height: default_window_height(),
            start_x: default_window_pos(),
            start_y: default_window_pos(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            fill_mode: default_fill_mode(),
            sung_color: default_sung_color(),
            unsung_color: default_unsung_color(),
            background_color: default_background_color(),
            font: FontConfig::default(),
            layout: LayoutConfig::default(),
            animation: AnimationConfig::default(),
            window: WindowConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandlingConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_ms: Vec<u64>,
    #[serde(default = "default_true")]
    pub show_error_notifications: bool,
}

const fn default_max_retries() -> u32 {
    3
}

fn default_retry_backoff() -> Vec<u64> {
    vec![100, 500, 2000]
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_backoff_ms: default_retry_backoff(),
            show_error_notifications: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WindowBehaviorConfig {
    #[serde(default = "default_true")]
    pub save_position: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_toggle_visibility")]
    pub toggle_visibility: String,
    #[serde(default = "default_quit")]
    pub quit: String,
}

fn default_toggle_visibility() -> String {
    "Ctrl+Shift+L".to_string()
}

fn default_quit() -> String {
    "Ctrl+Shift+Q".to_string()
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle_visibility: default_toggle_visibility(),
            quit: default_quit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "default_no_lyrics_behavior")]
    pub no_lyrics_behavior: String,
}

fn default_no_lyrics_behavior() -> String {
    "hide".to_string()
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            no_lyrics_behavior: default_no_lyrics_behavior(),
        }
    }
}

impl Config {
    /// Get the configuration directory path (~/.config/versualizer/)
    #[must_use] 
    pub fn config_dir() -> PathBuf {
        crate::paths::config_dir()
    }

    /// Get the config file path (~/.config/versualizer/config.toml)
    #[must_use] 
    pub fn config_path() -> PathBuf {
        crate::paths::config_path()
    }

    /// Load config from file or create template on first run
    ///
    /// # Errors
    ///
    /// Returns an error if the config file cannot be read, parsed, or if required fields are missing.
    pub fn load_or_create() -> Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            // Create config directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Write template config
            fs::write(&config_path, CONFIG_TEMPLATE)?;

            return Err(CoreError::ConfigNotFound { path: config_path });
        }

        let content = fs::read_to_string(&config_path)?;
        let config: Self = toml::from_str(&content)?;

        // Validate required fields
        if config.spotify.client_id.is_empty() {
            return Err(CoreError::ConfigMissingField {
                field: "spotify.client_id".to_string(),
            });
        }
        if config.spotify.client_secret.is_empty() {
            return Err(CoreError::ConfigMissingField {
                field: "spotify.client_secret".to_string(),
            });
        }

        Ok(config)
    }

    /// Parse a hex color string to RGB tuple
    #[must_use] 
    pub fn parse_color(hex: &str) -> Option<(u8, u8, u8, u8)> {
        let hex = hex.trim_start_matches('#');
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some((r, g, b, 255))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some((r, g, b, a))
            }
            _ => None,
        }
    }
}

const CONFIG_TEMPLATE: &str = r##"# Versualizer Configuration
# ~/.config/versualizer/config.toml

[spotify]
# Required: Get these from https://developer.spotify.com/dashboard
client_id = ""
client_secret = ""
oauth_redirect_uri = "http://127.0.0.1:8888/callback"
poll_interval_ms = 1000
# Optional: For unofficial Spotify lyrics API (use at your own risk - may violate TOS)
# sp_dc = ""

[lyrics]
# Provider priority: "spotify_lyrics", "lrclib"
# Providers are tried in order; first successful result wins
providers = ["lrclib"]
cache_enabled = true
cache_ttl_days = 30

[ui]
# Fill mode: "character" or "gradient"
fill_mode = "gradient"
sung_color = "#00FF00"
unsung_color = "#FFFFFF"
background_color = "#00000000"  # Transparent

[ui.font]
family = "Arial"
size = 24
weight = "normal"  # "thin", "normal", "bold"
fallbacks = ["Noto Sans CJK", "Segoe UI Emoji"]

[ui.layout]
alignment = "center"  # "left", "center", "right"
vertical_position = "bottom"  # "top", "center", "bottom"
margin_horizontal = 20
margin_vertical = 10
max_lines = 3
current_line_scale = 1.2

[ui.animation]
frame_rate = 60
transition_ms = 200
easing = "ease_in_out"  # "linear", "ease_in", "ease_out", "ease_in_out"

[ui.window]
width = 800
height = 200
start_x = -1  # -1 = centered
start_y = -1

[error_handling]
max_retries = 3
retry_backoff_ms = [100, 500, 2000]
show_error_notifications = true

[window]
# Window position persistence
save_position = true

[hotkeys]
# User-configurable hotkeys (format: "Modifier+Modifier+Key")
# Modifiers: Ctrl, Shift, Alt, Super (Win/Cmd)
toggle_visibility = "Ctrl+Shift+L"
quit = "Ctrl+Shift+Q"

[behavior]
# What to do when no lyrics are found: "hide" or "show_message"
no_lyrics_behavior = "hide"
"##;
