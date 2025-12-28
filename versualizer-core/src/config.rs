use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersualizerConfig {
    pub spotify: SpotifyConfig,
    pub lyrics: LyricsConfig,
    pub ui: UiConfig,
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
}

fn default_providers() -> Vec<LyricsProviderType> {
    vec![LyricsProviderType::Lrclib]
}

impl Default for LyricsConfig {
    fn default() -> Self {
        Self {
            providers: default_providers(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsProviderType {
    Lrclib,
    SpotifyLyrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub animation: AnimationConfig,
    #[serde(default)]
    pub window: WindowConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
    #[serde(default = "default_current_line_scale")]
    pub current_line_scale: f32,
    #[serde(default = "default_upcoming_line_scale")]
    pub upcoming_line_scale: f32,
}

const fn default_max_lines() -> usize {
    3
}

const fn default_current_line_scale() -> f32 {
    1.0
}

const fn default_upcoming_line_scale() -> f32 {
    0.8
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            max_lines: default_max_lines(),
            current_line_scale: default_current_line_scale(),
            upcoming_line_scale: default_upcoming_line_scale(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationConfig {
    #[serde(default = "default_animation_framerate")]
    pub framerate: u32,
    #[serde(default = "default_transition_ms")]
    pub transition_ms: u32,
    #[serde(default = "default_easing")]
    pub easing: String,
}

const fn default_animation_framerate() -> u32 {
    60
}

const fn default_transition_ms() -> u32 {
    200
}

fn default_easing() -> String {
    "ease-in-out".to_string()
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            framerate: default_animation_framerate(),
            transition_ms: default_transition_ms(),
            easing: default_easing(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_window_width")]
    pub width_px: u32,
    #[serde(default = "default_window_height")]
    pub height_px: u32,
}

const fn default_window_width() -> u32 {
    800
}

const fn default_window_height() -> u32 {
    200
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width_px: default_window_width(),
            height_px: default_window_height(),
        }
    }
}

impl VersualizerConfig {
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

        // Clamp max_lines to valid range (1-3)
        let mut config = config;
        config.ui.layout.max_lines = config.ui.layout.max_lines.clamp(1, 3);

        Ok(config)
    }
}

const CONFIG_TEMPLATE: &str = r#"# Versualizer Configuration
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

[ui.layout]
# The number of song lines to display in the visualizer
max_lines = 3
# Scale factor for the current (highlighted) line being sung
current_line_scale = 1.0
# Scale factor for upcoming lines to be sung
upcoming_line_scale = 0.8

[ui.animation]
# Animation framerate in frames per second
framerate = 60
transition_ms = 200
# CSS easing function for transitions https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function
easing = "ease-in-out"

[ui.window]
width_px = 800
height_px = 200
"#;
