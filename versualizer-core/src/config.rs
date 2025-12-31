use crate::error::{CoreError, Result};
use crate::source::MusicSource;
use const_format::concatcp;
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
    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
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
}

/// Logging configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Enable file logging to cache directory
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,
}

const DEFAULT_MAX_LINES: usize = 3;

const fn default_max_lines() -> usize {
    DEFAULT_MAX_LINES
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_MAX_LINES,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationConfig {
    #[serde(default = "default_animation_framerate")]
    pub framerate: u32,
    /// Drift threshold in milliseconds. If local and server playback positions differ
    /// by more than this amount, a hard sync is performed. Otherwise, the local timer
    /// is trusted to avoid unnecessary visual jumps.
    #[serde(default = "default_drift_threshold_ms")]
    pub drift_threshold_ms: u64,
}

const DEFAULT_ANIMATION_FRAMERATE: u32 = 60;
const DEFAULT_DRIFT_THRESHOLD_MS: u64 = 200;

const fn default_animation_framerate() -> u32 {
    DEFAULT_ANIMATION_FRAMERATE
}

const fn default_drift_threshold_ms() -> u64 {
    DEFAULT_DRIFT_THRESHOLD_MS
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            framerate: DEFAULT_ANIMATION_FRAMERATE,
            drift_threshold_ms: DEFAULT_DRIFT_THRESHOLD_MS,
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

            // Build and write template config
            let template = build_config_template(provider_templates);
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

/// Build the config template string with optional provider-specific sections.
///
/// This is useful for creating a fresh config file or resetting to defaults.
#[must_use]
pub fn build_config_template(provider_templates: Option<&[&str]>) -> String {
    let mut template = CONFIG_TEMPLATE_BASE.to_string();
    if let Some(templates) = provider_templates {
        for provider_template in templates {
            template.push_str(provider_template);
        }
    }
    template.push_str(CONFIG_TEMPLATE_UI);
    template
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

[logging]
# Enable file logging to cache directory (versualizer.log)
enabled = false

"#;

/// UI config template
const CONFIG_TEMPLATE_UI: &str = concatcp!(
    "[ui.layout]\n",
    "# The number of song lines to display in the visualizer\n",
    "max_lines = ",
    DEFAULT_MAX_LINES,
    "\n",
    "\n",
    "[ui.animation]\n",
    "# Animation framerate in frames per second\n",
    "framerate = ",
    DEFAULT_ANIMATION_FRAMERATE,
    "\n",
    "# Drift threshold in milliseconds. If local and server playback positions differ\n",
    "# by more than this amount, a hard sync is performed. Lower values = more syncs\n",
    "# but potential visual jumps. Higher values = fewer syncs but may drift.\n",
    "# Default ",
    DEFAULT_DRIFT_THRESHOLD_MS,
    "ms tolerates ~2-3 poll intervals while keeping lyrics visually in sync.\n",
    "drift_threshold_ms = ",
    DEFAULT_DRIFT_THRESHOLD_MS,
    "\n",
);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_music_config_default() {
        let config = MusicConfig::default();
        assert_eq!(config.source, MusicSource::Spotify);
    }

    #[test]
    fn test_lyrics_config_default() {
        let config = LyricsConfig::default();
        assert_eq!(config.providers, vec![LyricsProviderType::Lrclib]);
    }

    #[test]
    fn test_layout_config_default() {
        let config = LayoutConfig::default();
        assert_eq!(config.max_lines, 3);
    }

    #[test]
    fn test_animation_config_default() {
        let config = AnimationConfig::default();
        assert_eq!(config.framerate, 60);
        assert_eq!(config.drift_threshold_ms, 200);
    }

    #[test]
    fn test_ui_config_default() {
        let config = UiConfig::default();
        assert_eq!(config.layout.max_lines, 3);
        assert_eq!(config.animation.framerate, 60);
    }

    #[test]
    fn test_providers_config_contains() {
        let mut providers = ProvidersConfig::default();
        providers.inner.insert(
            "spotify".to_string(),
            toml::Value::Table(toml::map::Map::new()),
        );

        assert!(providers.contains("spotify"));
        assert!(!providers.contains("unknown"));
    }

    #[test]
    fn test_providers_config_get_none() {
        let providers = ProvidersConfig::default();
        let result: Result<Option<String>> = providers.get("unknown");

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_lyrics_provider_type_in_config() {
        // Test serialization/deserialization within a config context
        let config_str = r#"
[music]
source = "spotify"

[lyrics]
providers = ["lrclib", "spotify_lyrics"]

[ui]
"#;

        let config: VersualizerConfig = toml::from_str(config_str).unwrap();
        assert_eq!(config.lyrics.providers[0], LyricsProviderType::Lrclib);
        assert_eq!(
            config.lyrics.providers[1],
            LyricsProviderType::SpotifyLyrics
        );
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
[music]
source = "spotify"

[lyrics]
providers = ["lrclib", "spotify_lyrics"]

[ui.layout]
max_lines = 2
current_line_scale = 1.2
upcoming_line_scale = 0.7

[ui.animation]
framerate = 30
drift_threshold_ms = 500
"#;

        let config: VersualizerConfig = toml::from_str(toml_str).unwrap();

        assert_eq!(config.music.source, MusicSource::Spotify);
        assert_eq!(config.lyrics.providers.len(), 2);
        assert_eq!(config.lyrics.providers[0], LyricsProviderType::Lrclib);
        assert_eq!(
            config.lyrics.providers[1],
            LyricsProviderType::SpotifyLyrics
        );
        assert_eq!(config.ui.layout.max_lines, 2);
        assert_eq!(config.ui.animation.framerate, 30);
        assert_eq!(config.ui.animation.drift_threshold_ms, 500);
    }

    #[test]
    fn test_config_with_defaults() {
        // Minimal config - should use defaults for missing fields
        // Note: ui section is required but its subsections have defaults
        let toml_str = r#"
[music]
source = "spotify"

[lyrics]
providers = ["lrclib"]

[ui]
"#;

        let config: VersualizerConfig = toml::from_str(toml_str).unwrap();

        // Check that defaults are applied
        assert_eq!(config.ui.layout.max_lines, 3);
        assert_eq!(config.ui.animation.framerate, 60);
    }
}
