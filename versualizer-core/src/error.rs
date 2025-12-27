use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VersualizerError {
    // Configuration errors
    #[error("Config file not found at {path}. A template has been created - please edit it with your Spotify credentials and restart.")]
    ConfigNotFound { path: PathBuf },

    #[error("Invalid config: {message}")]
    ConfigInvalid { message: String },

    #[error("Missing required config field: {field}")]
    ConfigMissingField { field: String },

    #[error("Failed to parse config file: {0}")]
    ConfigParseError(#[from] toml::de::Error),

    // Spotify errors
    #[error("Spotify authentication failed: {reason}")]
    SpotifyAuthFailed { reason: String },

    #[error("Spotify token expired and refresh failed")]
    SpotifyTokenExpired,

    #[error("Spotify API rate limited, retry after {retry_after_secs}s")]
    SpotifyRateLimited { retry_after_secs: u32 },

    #[error("Spotify playback not active on any device")]
    SpotifyNoActivePlayback,

    // Lyrics errors
    #[error("Lyrics not found for track: {track} by {artist}")]
    LyricsNotFound { track: String, artist: String },

    #[error("Lyrics provider {provider} failed: {reason}")]
    LyricsProviderFailed { provider: String, reason: String },

    #[error("Failed to parse LRC: {reason}")]
    LrcParseError { reason: String },

    // Cache errors
    #[error("Cache database error: {0}")]
    CacheError(#[from] tokio_rusqlite::Error),

    #[error("SQLite error: {0}")]
    SqliteError(#[from] rusqlite::Error),

    // Network errors
    #[error("Network request failed: {0}")]
    NetworkError(#[from] reqwest::Error),

    // IO errors
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    // UI errors
    #[error("Window creation failed: {reason}")]
    WindowError { reason: String },

    #[error("Rendering error: {reason}")]
    RenderError { reason: String },
}

pub type Result<T> = std::result::Result<T, VersualizerError>;
