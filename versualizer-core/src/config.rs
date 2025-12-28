use crate::error::{CoreError, Result};
use crate::source::MusicSource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Main configuration structure (source-agnostic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersualizerConfig {
    /// Music configuration (source selection)
    pub music: MusicConfig,
    /// Lyrics provider configuration
    pub lyrics: LyricsConfig,
    /// UI configuration
    pub ui: UiConfig,
    /// Provider-specific configurations (dynamic)
    #[serde(default)]
    pub providers: ProvidersConfig,
}

/// Music configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MusicConfig {
    /// The active music source
    pub source: MusicSource,
}

impl Default for MusicConfig {
    fn default() -> Self {
        Self {
            source: MusicSource::Spotify,
        }
    }
}

/// Provider-specific configurations stored as dynamic TOML values.
/// Each provider crate defines its own config structure and parses from this.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProvidersConfig {
    /// Map of provider name to provider-specific configuration
    pub inner: HashMap<String, toml::Value>,
}

impl ProvidersConfig {
    /// Get a provider's configuration as a typed struct.
    ///
    /// # Errors
    ///
    /// Returns an error if the provider config cannot be deserialized into type `T`.
    pub fn get<T: serde::de::DeserializeOwned>(&self, provider: &str) -> Result<Option<T>> {
        self.inner.get(provider).map_or(Ok(None), |value| {
            value
                .clone()
                .try_into()
                .map(Some)
                .map_err(|e: toml::de::Error| CoreError::ConfigInvalid {
                    message: format!("Failed to parse {provider} config: {e}"),
                })
        })
    }

    /// Check if a provider configuration exists
    #[must_use]
    pub fn contains(&self, provider: &str) -> bool {
        self.inner.contains_key(provider)
    }
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

    /// Load config from file or create template on first run.
    ///
    /// This method loads the core configuration. Provider-specific validation
    /// should be done by the application after loading.
    ///
    /// # Arguments
    ///
    /// * `provider_templates` - Optional slice of provider config templates to append
    ///   to the base config template when creating a new config file.
    ///
    /// # Errors
    ///
    /// Returns an error if the config file cannot be read or parsed.
    pub fn load_or_create(provider_templates: Option<&[&str]>) -> Result<Self> {
        let config_path = Self::config_path();

        if !config_path.exists() {
            // Create config directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Build template with provider sections
            let mut template = CONFIG_TEMPLATE_BASE.to_string();
            if let Some(templates) = provider_templates {
                for provider_template in templates {
                    template.push_str(provider_template);
                }
            }
            template.push_str(CONFIG_TEMPLATE_UI);

            // Write template config
            fs::write(&config_path, template)?;

            return Err(CoreError::ConfigNotFound { path: config_path });
        }

        let content = fs::read_to_string(&config_path)?;
        let config: Self = toml::from_str(&content)?;

        // Clamp max_lines to valid range (1-3)
        let mut config = config;
        config.ui.layout.max_lines = config.ui.layout.max_lines.clamp(1, 3);

        Ok(config)
    }
}

/// Base config template (source-agnostic)
const CONFIG_TEMPLATE_BASE: &str = r#"# Versualizer Configuration
# ~/.config/versualizer/config.toml

[music]
# Active music source: "spotify", "mpris", "windows_media", "youtube_music"
source = "spotify"

[lyrics]
# Provider priority: providers are tried in order
# Available: "lrclib", "spotify_lyrics"
providers = ["lrclib"]

"#;

/// UI config template
const CONFIG_TEMPLATE_UI: &str = r#"[ui.layout]
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
# CSS easing function for transitions
# https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/easing-function
easing = "ease-in-out"

[ui.window]
width_px = 800
height_px = 200
"#;
